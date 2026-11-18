//! The job queue and lifecycle (spec §6.4, §7).
pub mod explain;
pub mod math;
pub mod notify;
pub mod progress;
pub mod runner;

use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::detect::{ext_of, Probe};
pub use crate::engines::EngineLease;
use crate::history::{Finish, History};
use crate::lockutil::lock;
use crate::platform;
use crate::presets::command::{build_command, expand, BuildInput};
use crate::presets::{naming, pandoc_reader, Catalog, Preset, ProgressKind};
use crate::settings::Settings;
use math::MB;
pub use runner::CancelToken;
use runner::{RunOutcome, RunSpec};

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    EngineInstallFailed,
    EngineFailed,
    Stalled,
    SourceUnreadable,
    DestUnwritable,
    DestLocked,
    DiskFull,
    TargetTooSmall,
    AlreadySmall,
    Cancelled,
    Interrupted,
}

impl ErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::EngineInstallFailed => "ENGINE_INSTALL_FAILED",
            Self::EngineFailed => "ENGINE_FAILED",
            Self::Stalled => "STALLED",
            Self::SourceUnreadable => "SOURCE_UNREADABLE",
            Self::DestUnwritable => "DEST_UNWRITABLE",
            Self::DestLocked => "DEST_LOCKED",
            Self::DiskFull => "DISK_FULL",
            Self::TargetTooSmall => "TARGET_TOO_SMALL",
            Self::AlreadySmall => "ALREADY_SMALL",
            Self::Cancelled => "CANCELLED",
            Self::Interrupted => "INTERRUPTED",
        }
    }
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct JobError {
    pub code: ErrorCode,
    pub message: String,
}

/// Payload of every job://* event (spec §6.4).
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct JobEvent {
    pub job_id: String,
    pub batch_id: String,
    pub batch_size: usize,
    pub preset_id: String,
    pub preset_label: String,
    /// Display name: first input's file name, plus " +N" for multi-file jobs.
    pub file: String,
    /// queued | waiting_engine | running | done | skipped | failed | cancelled
    pub status: String,
    pub pct: Option<f32>,
    pub eta_s: Option<f64>,
    pub out_path: Option<String>,
    pub error: Option<JobError>,
}

pub trait Events: Send + Sync {
    fn emit(&self, ev: &JobEvent);
}

/// What jobs need from engines (implemented by `Engines`; faked in tests).
pub trait EngineHost: Send + Sync {
    fn is_installed(&self, engine: &str) -> bool;
    fn ensure(&self, engine: &str, progress: &mut dyn FnMut(f32)) -> Result<(), String>;
    /// Shared use of the engine: it can't be removed or replaced while the lease lives.
    fn acquire(&self, engine: &str) -> EngineLease;
    /// Would `acquire` return at once? False while Library installs or removes the engine.
    fn lease_available(&self, _engine: &str) -> bool {
        true
    }
    fn bin(&self, engine: &str, bin: &str) -> Option<PathBuf>;
    fn probe(&self, path: &Path) -> Option<Probe>;
    fn pdf_pages(&self, path: &Path) -> Option<u32>;
}

pub type SettingsFn = Arc<dyn Fn() -> Settings + Send + Sync>;

pub fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string())
}

#[derive(Clone)]
struct Job {
    id: String,
    batch_id: String,
    batch_size: usize,
    preset_id: String,
    inputs: Vec<PathBuf>,
    fallback: bool,
}

impl Job {
    fn display(&self) -> String {
        let first = file_name(&self.inputs[0]);
        if self.inputs.len() > 1 { format!("{first} +{}", self.inputs.len() - 1) } else { first }
    }
}

#[derive(Default)]
struct State {
    queue: VecDeque<Job>,
    running: HashMap<String, CancelToken>,
}

struct Inner {
    catalog: Arc<Catalog>,
    host: Arc<dyn EngineHost>,
    history: Arc<Mutex<History>>,
    settings: SettingsFn,
    events: Arc<dyn Events>,
    /// %LOCALAPPDATA%\Recast\tmp: temp dirs for destinations whose path is too long.
    scratch: PathBuf,
    state: Mutex<State>,
}

#[derive(Clone)]
pub struct JobManager {
    inner: Arc<Inner>,
}

enum Finished {
    Done(PathBuf),
    Skipped(String),
}

#[derive(Debug)]
struct Failure {
    error: JobError,
    tail: Vec<String>,
}

impl Failure {
    fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { error: JobError { code, message: message.into() }, tail: Vec::new() }
    }
}

/// Deletes the job's temp dir however the job ends (outputs are moved out first).
/// History forgets the dir only once it is gone, so startup recovery retries otherwise.
struct TmpGuard {
    dir: PathBuf,
    history: Arc<Mutex<History>>,
    job_id: String,
}

impl Drop for TmpGuard {
    fn drop(&mut self) {
        for _ in 0..10 {
            match fs::remove_dir_all(&self.dir) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => std::thread::sleep(Duration::from_millis(100)),
                _ => {
                    let _ = lock(&self.history).set_temp(&self.job_id, None);
                    return;
                }
            }
        }
        log::warn!("couldn't remove temp dir {}", self.dir.display());
    }
}

/// Frees the job's concurrency slot and starts the next job, even if the job panicked.
struct Slot {
    inner: Arc<Inner>,
    job_id: String,
}

impl Drop for Slot {
    fn drop(&mut self) {
        lock(&self.inner.state).running.remove(&self.job_id);
        Inner::pump(&self.inner);
    }
}

impl JobManager {
    pub fn new(
        catalog: Arc<Catalog>,
        host: Arc<dyn EngineHost>,
        history: Arc<Mutex<History>>,
        settings: SettingsFn,
        events: Arc<dyn Events>,
        scratch: PathBuf,
    ) -> Self {
        Self { inner: Arc::new(Inner { catalog, host, history, settings, events, scratch, state: Mutex::new(State::default()) }) }
    }

    /// One job per file, or one job for all files when the preset is multi-file.
    pub fn start_jobs(&self, preset_id: &str, paths: &[PathBuf]) -> Result<Vec<String>, String> {
        self.enqueue(preset_id, paths, false)
    }

    /// Start queued jobs if there's room (call after the concurrency setting changes).
    pub fn pump(&self) {
        Inner::pump(&self.inner);
    }

    pub fn retry(&self, id: &str, fallback: bool) -> Result<Vec<String>, String> {
        let rec = lock(&self.inner.history).get(id).map_err(|e| e.to_string())?.ok_or("That job is no longer in the list.")?;
        let paths: Vec<PathBuf> = rec.inputs.iter().map(PathBuf::from).collect();
        self.enqueue(&rec.preset_id, &paths, fallback)
    }

    pub fn cancel(&self, id: &str) {
        let mut st = lock(&self.inner.state);
        if let Some(pos) = st.queue.iter().position(|j| j.id == id) {
            let job = st.queue.remove(pos).expect("position is valid");
            drop(st);
            let _ = lock(&self.inner.history).finish(
                &job.id,
                &Finish { status: "cancelled", error_code: Some("CANCELLED"), ..Default::default() },
                now(),
            );
            self.inner.emit(&job, "cancelled", None, None, None, Some(JobError { code: ErrorCode::Cancelled, message: "Cancelled.".into() }));
        } else if let Some(token) = st.running.get(id) {
            token.cancel();
        }
    }

    pub fn is_active(&self, id: &str) -> bool {
        let st = lock(&self.inner.state);
        st.running.contains_key(id) || st.queue.iter().any(|j| j.id == id)
    }

    fn enqueue(&self, preset_id: &str, paths: &[PathBuf], fallback: bool) -> Result<Vec<String>, String> {
        let preset = self.inner.catalog.get(preset_id).ok_or_else(|| format!("Unknown conversion '{preset_id}'."))?;
        if paths.is_empty() {
            return Err("No files to convert.".into());
        }
        let abs: Vec<PathBuf> = paths.iter().map(std::path::absolute).collect::<io::Result<_>>().map_err(|e| e.to_string())?;
        let groups: Vec<Vec<PathBuf>> = if preset.is_multi() { vec![abs] } else { abs.into_iter().map(|p| vec![p]).collect() };
        let batch_id = uuid::Uuid::new_v4().to_string();
        let batch_size = groups.len();
        let mut ids = Vec::new();
        for inputs in groups {
            let job = Job { id: uuid::Uuid::new_v4().to_string(), batch_id: batch_id.clone(), batch_size, preset_id: preset_id.to_string(), inputs, fallback };
            let inputs_str: Vec<String> = job.inputs.iter().map(|p| p.to_string_lossy().into_owned()).collect();
            lock(&self.inner.history).insert(&job.id, preset_id, &inputs_str, now()).map_err(|e| e.to_string())?;
            self.inner.emit(&job, "queued", None, None, None, None);
            ids.push(job.id.clone());
            lock(&self.inner.state).queue.push_back(job);
        }
        Inner::pump(&self.inner);
        Ok(ids)
    }
}

impl Inner {
    fn pump(this: &Arc<Inner>) {
        loop {
            let limit = (this.settings)().jobs.concurrency.max(1) as usize;
            let (job, token) = {
                let mut st = lock(&this.state);
                if st.running.len() >= limit {
                    return;
                }
                let Some(job) = st.queue.pop_front() else { return };
                let token = CancelToken::default();
                st.running.insert(job.id.clone(), token.clone());
                (job, token)
            };
            let me = this.clone();
            std::thread::spawn(move || {
                let _slot = Slot { inner: me.clone(), job_id: job.id.clone() };
                if catch_unwind(AssertUnwindSafe(|| me.execute(&job, &token))).is_err() {
                    me.internal_error(&job);
                }
            });
        }
    }

    /// A bug panicked the job thread: record the job as failed rather than leaving it
    /// "running" forever.
    fn internal_error(&self, job: &Job) {
        log::error!("job {} ({}) panicked", job.id, job.preset_id);
        let h = lock(&self.history);
        // A panic after the result was recorded (e.g. while notifying) changes nothing.
        let status = h.get(&job.id).ok().flatten().map(|r| r.status).unwrap_or_default();
        if matches!(status.as_str(), "done" | "skipped" | "failed" | "cancelled") {
            return;
        }
        let message = "Recast hit an internal error.";
        let _ = h.finish(
            &job.id,
            &Finish { status: "failed", error_code: Some(ErrorCode::EngineFailed.as_str()), error_detail: Some(message), ..Default::default() },
            now(),
        );
        drop(h);
        self.emit(job, "failed", None, None, None, Some(JobError { code: ErrorCode::EngineFailed, message: message.into() }));
    }

    fn emit(&self, job: &Job, status: &str, pct: Option<f32>, eta: Option<f64>, out: Option<String>, error: Option<JobError>) {
        self.events.emit(&JobEvent {
            job_id: job.id.clone(),
            batch_id: job.batch_id.clone(),
            batch_size: job.batch_size,
            preset_id: job.preset_id.clone(),
            preset_label: self.catalog.get(&job.preset_id).map(|p| p.label.clone()).unwrap_or_default(),
            file: job.display(),
            status: status.to_string(),
            pct,
            eta_s: eta,
            out_path: out,
            error,
        });
    }

    fn execute(&self, job: &Job, cancel: &CancelToken) {
        let mut started = Instant::now();
        let mut last_emit = Instant::now() - Duration::from_secs(1);
        let mut last_status = String::new();
        let mut progress = |status: &str, pct: Option<f32>| {
            let changed = status != last_status;
            if !changed && last_emit.elapsed() < Duration::from_millis(100) {
                return;
            }
            if changed {
                let _ = lock(&self.history).set_status(&job.id, status);
                last_status = status.to_string();
                if status == "running" {
                    // The ETA covers the conversion, not the engine download before it.
                    started = Instant::now();
                }
            }
            last_emit = Instant::now();
            let eta = (status == "running")
                .then_some(pct)
                .flatten()
                .filter(|p| *p > 0.02 && *p < 1.0)
                .map(|p| started.elapsed().as_secs_f64() * (1.0 - p as f64) / p as f64);
            self.emit(job, status, pct, eta, None, None);
        };
        progress("running", Some(0.0));
        let result = self.run_job(job, cancel, &mut progress);
        let h = lock(&self.history);
        match result {
            Ok(Finished::Done(out)) => {
                let o = out.to_string_lossy().into_owned();
                let _ = h.finish(&job.id, &Finish { status: "done", output: Some(&o), ..Default::default() }, now());
                drop(h);
                self.emit(job, "done", Some(1.0), None, Some(o), None);
            }
            Ok(Finished::Skipped(msg)) => {
                let _ = h.finish(&job.id, &Finish { status: "skipped", error_code: Some("ALREADY_SMALL"), error_detail: Some(&msg), ..Default::default() }, now());
                drop(h);
                self.emit(job, "skipped", None, None, None, Some(JobError { code: ErrorCode::AlreadySmall, message: msg }));
            }
            Err(f) => {
                let status = if f.error.code == ErrorCode::Cancelled { "cancelled" } else { "failed" };
                let tail = f.tail.join("\n");
                let _ = h.finish(
                    &job.id,
                    &Finish {
                        status,
                        error_code: Some(f.error.code.as_str()),
                        error_detail: Some(&f.error.message),
                        stderr_tail: (!tail.is_empty()).then_some(tail.as_str()),
                        ..Default::default()
                    },
                    now(),
                );
                drop(h);
                if status == "failed" {
                    log::warn!("job {} ({}) failed: {} {}", job.id, job.preset_id, f.error.code.as_str(), f.error.message);
                }
                self.emit(job, status, None, None, None, Some(f.error));
            }
        }
        let _ = lock(&self.history).prune(500);
    }

    fn dest_dir(&self, s: &Settings, job: &Job) -> Result<PathBuf, Failure> {
        let dir = if job.fallback {
            s.fallback_dir()
        } else if s.output.location == "folder" && !s.output.folder.trim().is_empty() {
            PathBuf::from(&s.output.folder)
        } else {
            job.inputs[0].parent().map(Path::to_path_buf).unwrap_or_default()
        };
        let unwritable = |e: io::Error| Failure::new(ErrorCode::DestUnwritable, format!("Recast can't save files in {}. ({e})", dir.display()));
        fs::create_dir_all(&dir).map_err(unwritable)?;
        let probe = dir.join(format!(".recast-probe-{}", job.id));
        fs::write(&probe, b"").map_err(unwritable)?;
        let _ = fs::remove_file(&probe);
        Ok(dir)
    }

    /// Install the engine and take a lease on it, on a helper thread so a cancelled
    /// job stops waiting at once. The install itself carries on (another job, or the
    /// next drop, will want it); a lease that arrives after cancel is dropped unused.
    fn ensure_engine(&self, engine: &str, cancel: &CancelToken, progress: &mut dyn FnMut(&str, Option<f32>)) -> Result<EngineLease, Failure> {
        enum Msg {
            Pct(f32),
            /// An install or update from Library holds the engine; the lease has to wait.
            Waiting,
            Done(Result<EngineLease, String>),
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let host = self.host.clone();
        let id = engine.to_string();
        std::thread::spawn(move || {
            let tx2 = tx.clone();
            let r = (|| {
                // Library may remove the engine between ensure and acquire: try again.
                for _ in 0..3 {
                    if !host.lease_available(&id) {
                        let _ = tx2.send(Msg::Waiting);
                    }
                    host.ensure(&id, &mut |p| {
                        let _ = tx2.send(Msg::Pct(p));
                    })?;
                    if !host.lease_available(&id) {
                        let _ = tx2.send(Msg::Waiting);
                    }
                    let lease = host.acquire(&id);
                    if host.is_installed(&id) {
                        return Ok(lease);
                    }
                }
                Err("the converter was removed while Recast was getting it ready".to_string())
            })();
            let _ = tx.send(Msg::Done(r));
        });
        loop {
            if cancel.is_cancelled() {
                return Err(Failure::new(ErrorCode::Cancelled, "Cancelled."));
            }
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(Msg::Pct(p)) => progress("waiting_engine", Some(p)),
                Ok(Msg::Waiting) => progress("waiting_engine", None),
                Ok(Msg::Done(Ok(lease))) => return Ok(lease),
                Ok(Msg::Done(Err(e))) => {
                    return Err(Failure::new(ErrorCode::EngineInstallFailed, format!("Couldn't download the converter: {e}")))
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(Failure::new(ErrorCode::EngineInstallFailed, "The converter download stopped unexpectedly."))
                }
            }
        }
    }

    fn run_job(&self, job: &Job, cancel: &CancelToken, progress: &mut dyn FnMut(&str, Option<f32>)) -> Result<Finished, Failure> {
        use ErrorCode::*;
        let settings = (self.settings)();
        let preset = self.catalog.get(&job.preset_id).ok_or_else(|| Failure::new(EngineFailed, "This conversion no longer exists."))?;
        let first = &job.inputs[0];

        // 1. Sources readable (SOURCE_UNREADABLE)
        let mut total_size = 0u64;
        for p in &job.inputs {
            let f = fs::File::open(p).map_err(|_| Failure::new(SourceUnreadable, format!("Couldn't read {}.", file_name(p))))?;
            total_size += f.metadata().map(|m| m.len()).unwrap_or(0);
        }

        // 2. Destination writable (DEST_UNWRITABLE)
        let dest = self.dest_dir(&settings, job)?;

        // 3. Engine present (ENGINE_INSTALL_FAILED)
        if !self.host.is_installed(&preset.engine) {
            progress("waiting_engine", Some(0.0));
        }
        // Held until the job ends, so Library can't remove or replace the engine mid-run.
        let _lease = self.ensure_engine(&preset.engine, cancel, progress)?;
        let program = self
            .host
            .bin(&preset.engine, &preset.bin)
            .ok_or_else(|| Failure::new(EngineInstallFailed, "The converter is missing its program. Reinstall it from Library."))?;
        // Leaving waiting_engine restarts the ETA clock (see execute).
        progress("running", Some(0.0));

        // 4. Facts for progress, Shrink and the disk check
        let probe = if preset.progress == ProgressKind::Ffmpeg || preset.uses("video_kbps") { self.host.probe(first) } else { None };
        let duration = probe.as_ref().and_then(|p| p.duration_s);
        let has_audio = probe.as_ref().map(|p| p.has_audio).unwrap_or(true);
        let pages = if preset.engine == "ghostscript" { self.host.pdf_pages(first) } else { None };
        let mut vars = settings.to_vars();
        vars.insert("reader".into(), pandoc_reader(&ext_of(first)));

        // 5. Shrink preconditions (ALREADY_SMALL, TARGET_TOO_SMALL)
        if preset.uses("video_kbps") {
            let target = settings.shrink.target_mb;
            if total_size <= target as u64 * MB {
                return Ok(Finished::Skipped(format!("{} is already under {target} MB.", file_name(first))));
            }
            let d = duration.ok_or_else(|| Failure::new(EngineFailed, "Couldn't read the video's length."))?;
            let kbps = math::shrink_video_kbps(target, d, has_audio).map_err(|min| {
                Failure::new(TargetTooSmall, format!("{target} MB is too small for this video. The smallest it can get is about {} MB.", min.ceil() as u64))
            })?;
            vars.insert("video_kbps".into(), kbps.to_string());
        }

        // 6. Disk space (DISK_FULL)
        let need = math::required_free(math::estimate_bytes(preset, total_size, duration, pages, settings.shrink.target_mb));
        if let Some(free) = platform::free_space(&dest)
            && free < need {
                return Err(Failure::new(DiskFull, format!("Not enough disk space: about {} MB is needed.", need / MB + 1)));
            }

        // 7. Hidden temp dir on the destination volume
        let tmp = tmp_dir_for(&dest, &job.id, &self.scratch)?;
        fs::create_dir_all(&tmp).map_err(|e| Failure::new(DestUnwritable, format!("Couldn't create a work folder: {e}")))?;
        let _ = lock(&self.history).set_temp(&job.id, Some(&tmp.to_string_lossy()));
        let _guard = TmpGuard { dir: tmp.clone(), history: self.history.clone(), job_id: job.id.clone() };
        let _ = platform::set_hidden(&tmp);

        let out_ext = preset.out_ext_for(first);
        // A name is taken if it exists, or if one of its extra outputs (e.g. Markdown's
        // "<stem>_media" folder) does: the .md links to that folder by name.
        let base_vars = vars.clone();
        let taken = |p: &Path| -> bool {
            if p.exists() {
                return true;
            }
            let stem = if preset.out_naming.is_folder() { p.file_name() } else { p.file_stem() };
            let mut v = base_vars.clone();
            v.insert("final_stem".into(), stem.map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
            preset.extra_outputs.iter().any(|x| expand(x, &v).map(|n| dest.join(n).exists()).unwrap_or(false))
        };
        let planned = naming::final_path(&dest, first, &preset.out_naming, &out_ext, &taken);
        let final_stem = if preset.out_naming.is_folder() { planned.file_name() } else { planned.file_stem() };
        vars.insert("final_stem".into(), final_stem.map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
        if preset.out_naming.is_folder() {
            fs::create_dir(tmp.join("out")).map_err(|e| Failure::new(DestUnwritable, e.to_string()))?;
        }
        let long_input = job.inputs.iter().any(|p| p.as_os_str().len() > 200);
        let inputs = if preset.stage_inputs || long_input { stage_inputs(&job.inputs, &tmp)? } else { job.inputs.clone() };
        let in_dir = first.parent().unwrap_or(Path::new("."));
        let passes = build_command(preset, &BuildInput { inputs: &inputs, in_dir, out_ext: &out_ext, vars: &vars })
            .map_err(|e| Failure::new(SourceUnreadable, e))?;

        // 8. Run every pass
        let n = passes.len();
        // ImageMagick reads Recast's policy.xml only through MAGICK_CONFIGURE_PATH.
        let env = if preset.engine == "imagemagick" { crate::engines::magick_env(&program) } else { Vec::new() };
        for (i, args) in passes.into_iter().enumerate() {
            let spec = RunSpec {
                program: program.clone(),
                args,
                cwd: tmp.clone(),
                progress: preset.progress,
                duration_s: duration,
                stall: Duration::from_secs(settings.jobs.stall_timeout_s as u64),
                hard_cap: Duration::from_secs(settings.jobs.no_progress_cap_min as u64 * 60 * job.inputs.len() as u64),
                env: env.clone(),
            };
            let res = runner::run(&spec, cancel, &mut |p| progress("running", Some((i as f32 + p) / n as f32)))
                .map_err(|e| Failure::new(EngineFailed, format!("Couldn't start the converter: {e}")))?;
            match res.outcome {
                RunOutcome::Exited(code) if preset.ok_exit.contains(&code) => {}
                RunOutcome::Exited(_) => {
                    let code = if explain::is_disk_full(&res.tail) { DiskFull } else { EngineFailed };
                    return Err(Failure { error: JobError { code, message: explain::explain(&res.tail) }, tail: res.tail });
                }
                RunOutcome::Stalled => {
                    return Err(Failure {
                        error: JobError { code: Stalled, message: "The converter stopped making progress, so Recast stopped it.".into() },
                        tail: res.tail,
                    })
                }
                RunOutcome::Cancelled => return Err(Failure::new(Cancelled, "Cancelled.")),
            }
        }

        // 9. Check and finalize
        let produced = if preset.out_naming.is_folder() { tmp.join("out") } else { tmp.join(format!("out.{out_ext}")) };
        if !produced.exists() {
            return Err(Failure::new(EngineFailed, "The converter finished but didn't write a file."));
        }
        if preset.discard_if_not_smaller && fs::metadata(&produced).map(|m| m.len()).unwrap_or(u64::MAX) >= total_size {
            return Ok(Finished::Skipped(format!("{} is already as small as it gets.", file_name(first))));
        }
        // Extra outputs (e.g. "<stem>_media") first: if anything fails they're moved
        // back into the temp dir, so a failure never leaves a half-finished result.
        let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
        let result = (|| {
            for extra in &preset.extra_outputs {
                let name = expand(extra, &vars).map_err(|e| Failure::new(EngineFailed, e))?;
                let src = tmp.join(&name);
                if src.exists() {
                    let target = naming::unique_path(&dest, &name, None, &|p| p.exists());
                    move_with_retry(&src, &target).map_err(map_move_err)?;
                    moved.push((src, target));
                }
            }
            // The extras we just moved belong to `planned`: they don't make it taken.
            let taken_now = |p: &Path| if p == planned { p.exists() } else { taken(p) };
            finalize(&produced, &dest, first, preset, &out_ext, &taken_now)
        })();
        if result.is_err() {
            for (src, target) in moved.iter().rev() {
                if let Err(e) = platform::move_no_replace(target, src) {
                    log::warn!("couldn't take back {}: {e}", target.display());
                }
            }
        }
        result.map(Finished::Done)
    }
}

/// `<dest>\.recast-<id>` normally; engines can't use a working directory past
/// MAX_PATH, so long destinations use the scratch root when it's on the same volume.
fn tmp_dir_for(dest: &Path, id: &str, scratch: &Path) -> Result<PathBuf, Failure> {
    let local = dest.join(format!(".recast-{id}"));
    if local.as_os_str().len() < 240 {
        return Ok(local);
    }
    let drive = |p: &Path| match p.components().next() {
        Some(Component::Prefix(pre)) => Some(pre.as_os_str().to_string_lossy().to_ascii_uppercase()),
        _ => None,
    };
    if drive(dest).is_some() && drive(dest) == drive(scratch) {
        Ok(scratch.join(format!(".recast-{id}")))
    } else {
        Err(Failure::new(ErrorCode::DestUnwritable, "This folder's path is too long for Recast. Move the file somewhere with a shorter path."))
    }
}

/// Hard-link (or copy, across volumes) inputs into the temp dir as in-N.<ext>.
fn stage_inputs(inputs: &[PathBuf], tmp: &Path) -> Result<Vec<PathBuf>, Failure> {
    inputs
        .iter()
        .enumerate()
        .map(|(i, src)| {
            let ext = ext_of(src);
            let name = if ext.is_empty() { format!("in-{}", i + 1) } else { format!("in-{}.{ext}", i + 1) };
            let dst = tmp.join(&name);
            fs::hard_link(src, &dst)
                .or_else(|_| fs::copy(src, &dst).map(|_| ()))
                .map_err(|e| Failure::new(ErrorCode::SourceUnreadable, format!("Couldn't read {}: {e}", file_name(src))))?;
            Ok(PathBuf::from(name))
        })
        .collect()
}

fn finalize(produced: &Path, dest: &Path, first: &Path, preset: &Preset, out_ext: &str, taken: &dyn Fn(&Path) -> bool) -> Result<PathBuf, Failure> {
    for _ in 0..100 {
        // Re-check collisions at rename time; move_no_replace makes the check race-free.
        let target = naming::final_path(dest, first, &preset.out_naming, out_ext, taken);
        match move_with_retry(produced, &target) {
            Ok(()) => return Ok(target),
            Err(e) if platform::is_already_exists(&e) => continue,
            Err(e) => return Err(map_move_err(e)),
        }
    }
    Err(Failure::new(ErrorCode::DestUnwritable, "Couldn't find a free file name."))
}

fn map_move_err(e: io::Error) -> Failure {
    if platform::is_transient_lock(&e) {
        Failure::new(ErrorCode::DestLocked, "Another program is holding the output file. Try again in a moment.")
    } else {
        Failure::new(ErrorCode::DestUnwritable, format!("Couldn't save the result: {e}"))
    }
}

/// Can a new file be created in `dir` right now?
fn dir_accepts_files(dir: &Path) -> bool {
    let probe = dir.join(format!(".recast-probe-{}", uuid::Uuid::new_v4()));
    match fs::File::create_new(&probe) {
        Ok(f) => {
            drop(f);
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// Access denied is a scanner's brief hold only while the folder still takes new
/// files; otherwise it is a permissions problem and retrying won't help.
fn access_denied_is_transient(to: &Path) -> bool {
    to.parent().is_some_and(dir_accepts_files)
}

/// Antivirus/OneDrive/Dropbox briefly lock new files: retry 100 ms, doubling, ≤ 5 s total.
fn move_with_retry(from: &Path, to: &Path) -> io::Result<()> {
    let mut wait = 100u64;
    let mut waited = 0u64;
    loop {
        match platform::move_no_replace(from, to) {
            Ok(()) => return Ok(()),
            // Without the raw code, map_move_err reports DEST_UNWRITABLE.
            Err(e) if e.raw_os_error() == Some(5) && !access_denied_is_transient(to) => {
                return Err(io::Error::new(io::ErrorKind::PermissionDenied, e.to_string()))
            }
            Err(e) if platform::is_transient_lock(&e) && waited < 5000 => {
                let d = wait.min(5000 - waited);
                std::thread::sleep(Duration::from_millis(d));
                waited += d;
                wait *= 2;
            }
            Err(e) => return Err(e),
        }
    }
}

impl EngineHost for crate::engines::Engines {
    fn is_installed(&self, engine: &str) -> bool {
        self.is_ready(engine)
    }
    fn ensure(&self, engine: &str, progress: &mut dyn FnMut(f32)) -> Result<(), String> {
        crate::engines::Engines::ensure(self, engine, progress)
    }
    fn acquire(&self, engine: &str) -> EngineLease {
        crate::engines::Engines::acquire(self, engine)
    }
    fn lease_available(&self, engine: &str) -> bool {
        crate::engines::Engines::lease_available(self, engine)
    }
    fn bin(&self, engine: &str, bin: &str) -> Option<PathBuf> {
        crate::engines::Engines::bin(self, engine, bin)
    }
    fn probe(&self, path: &Path) -> Option<Probe> {
        let ffprobe = crate::engines::Engines::bin(self, "ffmpeg", "ffprobe")?;
        let args = [
            std::ffi::OsStr::new("-v"),
            std::ffi::OsStr::new("error"),
            std::ffi::OsStr::new("-print_format"),
            std::ffi::OsStr::new("json"),
            std::ffi::OsStr::new("-show_format"),
            std::ffi::OsStr::new("-show_streams"),
            path.as_os_str(),
        ];
        let out = crate::proc::run_capture(&ffprobe, args, Duration::from_secs(15))?;
        crate::detect::parse_ffprobe_json(&out)
    }
    fn pdf_pages(&self, path: &Path) -> Option<u32> {
        let qpdf = crate::engines::Engines::bin(self, "qpdf", "qpdf")?;
        let out = crate::proc::run_capture(&qpdf, [std::ffi::OsStr::new("--show-npages"), path.as_os_str()], Duration::from_secs(15))?;
        out.trim().parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::fake_engine;
    use std::sync::Condvar;

    const CATALOG: &str = r#"
[[preset]]
id = "ok"
label = "OK"
description = "d"
icon = "i"
kinds = ["unknown"]
engine = "fake"
bin = "fake"
progress = "ffmpeg"
out_ext = "txt"
out_naming = "replace_ext"
passes = [['ok', '{out}']]

[[preset]]
id = "fail"
label = "Fail"
description = "d"
icon = "i"
kinds = ["unknown"]
engine = "fake"
bin = "fake"
out_ext = "txt"
out_naming = "replace_ext"
passes = [['fail']]

[[preset]]
id = "hang"
label = "Hang"
description = "d"
icon = "i"
kinds = ["unknown"]
engine = "fake"
bin = "fake"
progress = "ffmpeg"
out_ext = "txt"
out_naming = "replace_ext"
passes = [['hang_after_progress']]

[[preset]]
id = "slow"
label = "Slow"
description = "d"
icon = "i"
kinds = ["unknown"]
engine = "fake"
bin = "fake"
out_ext = "txt"
out_naming = "replace_ext"
passes = [['hang']]

[[preset]]
id = "pages"
label = "Pages"
description = "d"
icon = "i"
kinds = ["unknown"]
engine = "fake"
bin = "fake"
out_ext = "txt"
out_naming = "folder:pages"
passes = [['write_dir', '{out_dir}']]

[[preset]]
id = "merge"
label = "Merge"
description = "d"
icon = "i"
kinds = ["unknown"]
min_files = 2
engine = "fake"
bin = "fake"
progress = "ffmpeg"
out_ext = "txt"
out_naming = "fixed:merged"
passes = [['ok', '{out}', '{inputs}']]

[[preset]]
id = "compress"
label = "Compress"
description = "d"
icon = "i"
kinds = ["unknown"]
engine = "fake"
bin = "fake"
keep_same_format = true
discard_if_not_smaller = true
out_ext = "same"
out_naming = "suffix:compressed"
passes = [['write_out', '{out}', '100000']]

[[preset]]
id = "shrink"
label = "Shrink"
description = "d"
icon = "i"
kinds = ["unknown"]
engine = "fake"
bin = "fake"
progress = "ffmpeg"
keep_same_format = true
out_ext = "same"
out_naming = "suffix:small"
passes = [['write_out', '{out}', '{video_kbps}']]

[[preset]]
id = "diskfull"
label = "Disk full"
description = "d"
icon = "i"
kinds = ["unknown"]
engine = "fake"
bin = "fake"
out_ext = "txt"
out_naming = "replace_ext"
passes = [['disk_full']]

[[preset]]
id = "md"
label = "Markdown"
description = "d"
icon = "i"
kinds = ["unknown"]
engine = "fake"
bin = "fake"
out_ext = "md"
out_naming = "replace_ext"
extra_outputs = ['{final_stem}_media']
passes = [['write_media', '{out}', '{final_stem}_media']]

[[preset]]
id = "boom"
label = "Boom"
description = "d"
icon = "i"
kinds = ["unknown"]
engine = "boom"
bin = "fake"
out_ext = "txt"
out_naming = "replace_ext"
passes = [['ok', '{out}']]

[[preset]]
id = "broken"
label = "Broken"
description = "d"
icon = "i"
kinds = ["unknown"]
engine = "broken"
bin = "fake"
out_ext = "txt"
out_naming = "replace_ext"
passes = [['ok', '{out}']]

[[preset]]
id = "leasewait"
label = "Lease wait"
description = "d"
icon = "i"
kinds = ["unknown"]
engine = "leasewait"
bin = "fake"
out_ext = "txt"
out_naming = "replace_ext"
passes = [['ok', '{out}']]

[[preset]]
id = "slowdl"
label = "Slow download"
description = "d"
icon = "i"
kinds = ["unknown"]
engine = "slowdl"
bin = "fake"
out_ext = "txt"
out_naming = "replace_ext"
passes = [['ok', '{out}']]
"#;

    /// Engines: "fake" works; "boom" panics in bin(); "broken" fails to install;
    /// "slowdl" takes seconds to install.
    /// "leasewait": installed, but its lease is taken (a Library update) until this is set.
    static LEASE_FREE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    struct FakeHost {
        exe: PathBuf,
        duration_s: f64,
        slow_done: std::sync::atomic::AtomicBool,
    }
    impl EngineHost for FakeHost {
        fn is_installed(&self, engine: &str) -> bool {
            match engine {
                "broken" => false,
                "slowdl" => self.slow_done.load(std::sync::atomic::Ordering::SeqCst),
                _ => true,
            }
        }
        fn ensure(&self, engine: &str, _: &mut dyn FnMut(f32)) -> Result<(), String> {
            match engine {
                "broken" => Err("network down".into()),
                "slowdl" => {
                    std::thread::sleep(Duration::from_secs(5));
                    self.slow_done.store(true, std::sync::atomic::Ordering::SeqCst);
                    Ok(())
                }
                _ => Ok(()),
            }
        }
        fn acquire(&self, engine: &str) -> EngineLease {
            while engine == "leasewait" && !LEASE_FREE.load(std::sync::atomic::Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(10));
            }
            EngineLease::noop()
        }
        fn lease_available(&self, engine: &str) -> bool {
            engine != "leasewait" || LEASE_FREE.load(std::sync::atomic::Ordering::SeqCst)
        }
        fn bin(&self, engine: &str, _: &str) -> Option<PathBuf> {
            assert_ne!(engine, "boom", "simulated bug");
            Some(self.exe.clone())
        }
        fn probe(&self, _: &Path) -> Option<Probe> {
            Some(Probe { duration_s: Some(self.duration_s), has_audio: true, has_video: true, audio_codec: None })
        }
        fn pdf_pages(&self, _: &Path) -> Option<u32> {
            None
        }
    }

    #[derive(Default)]
    pub(crate) struct Collect {
        evs: Mutex<Vec<JobEvent>>,
        cv: Condvar,
    }
    impl Events for Collect {
        fn emit(&self, ev: &JobEvent) {
            self.evs.lock().unwrap().push(ev.clone());
            self.cv.notify_all();
        }
    }
    const TERMINAL: &[&str] = &["done", "skipped", "failed", "cancelled"];
    impl Collect {
        fn wait_for(&self, id: &str, statuses: &[&str]) -> JobEvent {
            let deadline = Instant::now() + Duration::from_secs(30);
            let mut g = self.evs.lock().unwrap();
            loop {
                if let Some(e) = g.iter().find(|e| e.job_id == id && statuses.contains(&e.status.as_str())) {
                    return e.clone();
                }
                let left = deadline.saturating_duration_since(Instant::now());
                assert!(!left.is_zero(), "job {id} never reached {statuses:?}: {:?}", *g);
                g = self.cv.wait_timeout(g, left).unwrap().0;
            }
        }
        fn wait_end(&self, id: &str) -> JobEvent {
            self.wait_for(id, TERMINAL)
        }
        fn index_of(&self, id: &str, status: &str) -> usize {
            self.evs.lock().unwrap().iter().position(|e| e.job_id == id && e.status == status).unwrap()
        }
    }

    struct Rig {
        mgr: JobManager,
        events: Arc<Collect>,
        history: Arc<Mutex<History>>,
        dir: tempfile::TempDir,
        _scratch: tempfile::TempDir,
    }

    fn rig_with(f: impl FnOnce(&mut Settings)) -> Rig {
        rig_full(f, 1.0)
    }
    fn rig_full(f: impl FnOnce(&mut Settings), duration_s: f64) -> Rig {
        let mut s = Settings::default();
        f(&mut s);
        let events = Arc::new(Collect::default());
        let history = Arc::new(Mutex::new(History::open_in_memory().unwrap()));
        let scratch = tempfile::tempdir().unwrap();
        let mgr = JobManager::new(
            Arc::new(Catalog::parse(CATALOG).unwrap()),
            Arc::new(FakeHost { exe: fake_engine(), duration_s, slow_done: Default::default() }),
            history.clone(),
            Arc::new(move || s.clone()),
            events.clone(),
            scratch.path().to_path_buf(),
        );
        Rig { mgr, events, history, dir: tempfile::tempdir().unwrap(), _scratch: scratch }
    }
    fn rig() -> Rig {
        rig_with(|_| {})
    }

    impl Rig {
        fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
            let p = self.dir.path().join(name);
            fs::write(&p, bytes).unwrap();
            p
        }
        fn names(&self) -> Vec<String> {
            let mut v: Vec<String> = fs::read_dir(self.dir.path()).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
            v.sort();
            v
        }
        fn run(&self, preset: &str, files: &[PathBuf]) -> JobEvent {
            let ids = self.mgr.start_jobs(preset, files).unwrap();
            self.events.wait_end(&ids[0])
        }
    }

    #[test]
    fn converts_next_to_the_source_and_cleans_up() {
        let r = rig();
        let src = r.file("a.bin", b"source");
        let ev = r.run("ok", &[src]);
        assert_eq!(ev.status, "done", "{:?}", ev.error);
        let out = PathBuf::from(ev.out_path.unwrap());
        assert_eq!(out.file_name().unwrap(), "a.txt");
        assert_eq!(fs::read_to_string(&out).unwrap(), "converted");
        assert_eq!(r.names(), ["a.bin", "a.txt"], "no .recast-* temp dir left");
        assert_eq!(ev.preset_label, "OK");
        let rec = r.history.lock().unwrap().get(&ev.job_id).unwrap().unwrap();
        assert_eq!(rec.status, "done");
        assert!(rec.temp_path.is_none());
        let saw_progress = r.events.evs.lock().unwrap().iter().any(|e| e.status == "running" && e.pct.unwrap_or(0.0) > 0.0);
        assert!(saw_progress);
    }

    #[test]
    fn never_overwrites() {
        let r = rig();
        let src = r.file("a.bin", b"source");
        r.file("a.txt", b"mine");
        let ev = r.run("ok", &[src]);
        assert!(ev.out_path.unwrap().ends_with("a (1).txt"));
        assert_eq!(fs::read_to_string(r.dir.path().join("a.txt")).unwrap(), "mine");
    }

    #[test]
    fn engine_failure_is_explained_and_leaves_nothing() {
        let r = rig();
        let src = r.file("a.bin", b"source");
        let ev = r.run("fail", &[src]);
        assert_eq!(ev.status, "failed");
        let err = ev.error.unwrap();
        assert_eq!(err.code, ErrorCode::EngineFailed);
        assert_eq!(err.message, "The file looks damaged or incomplete.");
        assert_eq!(r.names(), ["a.bin"]);
        let rec = r.history.lock().unwrap().get(&ev.job_id).unwrap().unwrap();
        assert!(rec.stderr_tail.unwrap().contains("Invalid data"));
        assert_eq!(rec.error_code.as_deref(), Some("ENGINE_FAILED"));
    }

    #[test]
    fn stalled_engine_is_killed() {
        let r = rig_with(|s| s.jobs.stall_timeout_s = 1);
        let src = r.file("a.bin", b"source");
        let t = Instant::now();
        let ev = r.run("hang", &[src]);
        assert_eq!(ev.error.unwrap().code, ErrorCode::Stalled);
        assert!(t.elapsed() < Duration::from_secs(10));
        assert_eq!(r.names(), ["a.bin"]);
    }

    #[test]
    fn cancel_running_and_queued_jobs() {
        let r = rig_with(|s| s.jobs.concurrency = 1);
        let a = r.file("a.bin", b"a");
        let b = r.file("b.bin", b"b");
        let first = r.mgr.start_jobs("slow", &[a]).unwrap().remove(0);
        let second = r.mgr.start_jobs("ok", &[b]).unwrap().remove(0);
        r.events.wait_for(&first, &["running"]);
        r.mgr.cancel(&second);
        assert_eq!(r.events.wait_end(&second).status, "cancelled");
        r.mgr.cancel(&first);
        assert_eq!(r.events.wait_end(&first).status, "cancelled");
        assert_eq!(r.names(), ["a.bin", "b.bin"]);
    }

    #[test]
    fn concurrency_one_runs_jobs_in_turn() {
        let r = rig_with(|s| s.jobs.concurrency = 1);
        let a = r.file("a.bin", b"a");
        let b = r.file("b.bin", b"b");
        let ids = r.mgr.start_jobs("ok", &[a, b]).unwrap();
        r.events.wait_end(&ids[0]);
        r.events.wait_end(&ids[1]);
        assert!(r.events.index_of(&ids[1], "running") > r.events.index_of(&ids[0], "done"));
    }

    #[test]
    fn missing_source_fails_before_spawning() {
        let r = rig();
        let ev = r.run("ok", &[r.dir.path().join("gone.bin")]);
        assert_eq!(ev.error.unwrap().code, ErrorCode::SourceUnreadable);
    }

    #[test]
    fn multi_file_preset_is_one_job() {
        let r = rig();
        let a = r.file("a.bin", b"a");
        let b = r.file("b.bin", b"b");
        let ids = r.mgr.start_jobs("merge", &[a, b]).unwrap();
        assert_eq!(ids.len(), 1);
        let ev = r.events.wait_end(&ids[0]);
        assert!(ev.out_path.unwrap().ends_with("merged.txt"));
        assert_eq!(ev.file, "a.bin +1");
    }

    #[test]
    fn folder_output() {
        let r = rig();
        let src = r.file("a.bin", b"a");
        let ev = r.run("pages", &[src]);
        let out = PathBuf::from(ev.out_path.unwrap());
        assert_eq!(out.file_name().unwrap(), "a (pages)");
        assert!(out.join("page-2.txt").is_file());
    }

    #[test]
    fn compress_that_isnt_smaller_is_skipped() {
        let r = rig();
        let src = r.file("a.bin", b"tiny");
        let ev = r.run("compress", &[src]);
        assert_eq!(ev.status, "skipped");
        assert_eq!(ev.error.unwrap().code, ErrorCode::AlreadySmall);
        assert_eq!(r.names(), ["a.bin"]);
    }

    #[test]
    fn retry_into_the_fallback_folder() {
        let fallback = tempfile::tempdir().unwrap();
        let fb = fallback.path().to_string_lossy().into_owned();
        let r = rig_with(move |s| s.output.fallback_folder = fb);
        let src = r.file("a.bin", b"a");
        let first = r.run("ok", &[src]);
        let again = r.mgr.retry(&first.job_id, true).unwrap().remove(0);
        let ev = r.events.wait_end(&again);
        assert!(PathBuf::from(ev.out_path.unwrap()).starts_with(fallback.path()));
    }

    #[test]
    fn a_panicking_job_fails_and_frees_its_slot() {
        let r = rig_with(|s| s.jobs.concurrency = 1);
        let a = r.file("a.bin", b"a");
        let b = r.file("b.bin", b"b");
        let ev = r.run("boom", &[a]);
        assert_eq!(ev.status, "failed");
        let err = ev.error.unwrap();
        assert_eq!(err.code, ErrorCode::EngineFailed);
        assert_eq!(err.message, "Recast hit an internal error.");
        let rec = r.history.lock().unwrap().get(&ev.job_id).unwrap().unwrap();
        assert_eq!(rec.status, "failed");
        assert_eq!(rec.error_code.as_deref(), Some("ENGINE_FAILED"));
        assert!(!r.mgr.is_active(&ev.job_id));
        assert_eq!(r.run("ok", &[b]).status, "done", "the slot was freed");
    }

    #[test]
    fn target_too_small_for_a_long_video() {
        let r = rig_full(|s| s.shrink.target_mb = 25, 3600.0);
        let src = r.dir.path().join("long.mp4");
        fs::File::create(&src).unwrap().set_len(26 * MB).unwrap();
        let ev = r.run("shrink", &[src]);
        assert_eq!(ev.status, "failed");
        let err = ev.error.unwrap();
        assert_eq!(err.code, ErrorCode::TargetTooSmall);
        assert!(err.message.starts_with("25 MB is too small"), "{}", err.message);
        assert_eq!(r.names(), ["long.mp4"]);
    }

    #[test]
    fn shrink_skips_a_file_already_under_the_target() {
        let r = rig_with(|s| s.shrink.target_mb = 25);
        let src = r.file("small.mp4", b"tiny");
        let ev = r.run("shrink", &[src]);
        assert_eq!(ev.status, "skipped");
        let err = ev.error.unwrap();
        assert_eq!(err.code, ErrorCode::AlreadySmall);
        assert_eq!(err.message, "small.mp4 is already under 25 MB.");
        assert_eq!(r.names(), ["small.mp4"]);
    }

    #[test]
    fn engine_install_failure_is_reported() {
        let r = rig();
        let src = r.file("a.bin", b"a");
        let ev = r.run("broken", &[src]);
        assert_eq!(ev.status, "failed");
        let err = ev.error.unwrap();
        assert_eq!(err.code, ErrorCode::EngineInstallFailed);
        assert!(err.message.contains("network down"), "{}", err.message);
        assert!(r.events.index_of(&ev.job_id, "waiting_engine") < r.events.index_of(&ev.job_id, "failed"));
    }

    #[test]
    fn a_job_waiting_on_an_engine_lease_says_so() {
        let r = rig();
        let src = r.file("a.bin", b"a");
        let id = r.mgr.start_jobs("leasewait", &[src]).unwrap().remove(0);
        let waiting = r.events.wait_for(&id, &["waiting_engine"]);
        assert_eq!(waiting.pct, None);
        LEASE_FREE.store(true, std::sync::atomic::Ordering::SeqCst);
        let end = r.events.wait_end(&id);
        assert_eq!(end.status, "done", "{:?}", end.error);
        assert!(r.events.index_of(&id, "waiting_engine") < r.events.index_of(&id, "done"));
    }

    #[test]
    fn cancel_while_waiting_for_an_engine_download() {
        let r = rig();
        let src = r.file("a.bin", b"a");
        let id = r.mgr.start_jobs("slowdl", &[src]).unwrap().remove(0);
        r.events.wait_for(&id, &["waiting_engine"]);
        let t = Instant::now();
        r.mgr.cancel(&id);
        assert_eq!(r.events.wait_end(&id).status, "cancelled");
        assert!(t.elapsed() < Duration::from_secs(2), "cancel must not wait for the download");
        assert_eq!(r.names(), ["a.bin"]);
    }

    #[test]
    fn disk_full_in_engine_output_is_disk_full() {
        let r = rig();
        let src = r.file("a.bin", b"a");
        let ev = r.run("diskfull", &[src]);
        assert_eq!(ev.status, "failed");
        let err = ev.error.unwrap();
        assert_eq!(err.code, ErrorCode::DiskFull);
        assert_eq!(err.message, "The disk is full.");
    }

    #[test]
    fn extra_outputs_land_beside_the_main_output() {
        let r = rig();
        let src = r.file("notes.docx", b"doc");
        let ev = r.run("md", &[src]);
        assert_eq!(ev.status, "done", "{:?}", ev.error);
        assert!(ev.out_path.unwrap().ends_with("notes.md"));
        assert!(r.dir.path().join("notes_media").join("media").join("x.png").is_file());
        assert_eq!(r.names(), ["notes.docx", "notes.md", "notes_media"]);
    }

    #[test]
    fn access_denied_counts_as_transient_only_while_the_folder_is_writable() {
        let d = tempfile::tempdir().unwrap();
        assert!(access_denied_is_transient(&d.path().join("out.txt")));
        assert!(!access_denied_is_transient(&d.path().join("missing").join("out.txt")));
        assert_eq!(fs::read_dir(d.path()).unwrap().count(), 0, "the probe file is removed");
    }

    #[test]
    fn temp_dir_stays_in_history_until_removed() {
        let r = rig();
        let src = r.file("a.bin", b"a");
        let ev = r.run("ok", &[src]);
        assert!(r.history.lock().unwrap().get(&ev.job_id).unwrap().unwrap().temp_path.is_none());
        // finish() alone no longer forgets a temp dir: only its removal does.
        let h = r.history.lock().unwrap();
        h.set_temp(&ev.job_id, Some(r"C:\x\.recast-left")).unwrap();
        h.finish(&ev.job_id, &Finish { status: "done", ..Default::default() }, now()).unwrap();
        assert_eq!(h.get(&ev.job_id).unwrap().unwrap().temp_path.as_deref(), Some(r"C:\x\.recast-left"));
    }

    #[test]
    fn a_running_job_holds_its_engine_lease() {
        use crate::engines::install::{tests::{entry, sevenzip}, LocalFetcher};
        use crate::engines::manifest::Manifest;
        use crate::engines::{Engines, ENGINE_BUSY};
        let d = tempfile::tempdir().unwrap();
        // An engine archive whose tool.exe is the fake engine.
        let pkg = d.path().join("pkg").join("tool");
        fs::create_dir_all(&pkg).unwrap();
        fs::copy(fake_engine(), pkg.join("tool.exe")).unwrap();
        let zip = d.path().join("tool-1.0.zip");
        let st = std::process::Command::new(sevenzip()).args(["a", "-tzip"]).arg(&zip).arg(d.path().join("pkg").join("*")).output().unwrap();
        assert!(st.status.success());
        let engines = Arc::new(Engines::new(
            Manifest { engines: vec![entry(&zip, "1.0")] },
            d.path().join("engines"),
            sevenzip(),
            Box::new(LocalFetcher { source: zip }),
        ));
        let catalog = r#"
[[preset]]
id = "tool-hang"
label = "Hang"
description = "d"
icon = "i"
kinds = ["unknown"]
engine = "tool"
bin = "tool"
out_ext = "txt"
out_naming = "replace_ext"
passes = [['hang']]
"#;
        let events = Arc::new(Collect::default());
        let mgr = JobManager::new(
            Arc::new(Catalog::parse(catalog).unwrap()),
            engines.clone(),
            Arc::new(Mutex::new(History::open_in_memory().unwrap())),
            Arc::new(Settings::default),
            events.clone(),
            d.path().join("scratch"),
        );
        let src = d.path().join("a.bin");
        fs::write(&src, b"a").unwrap();
        let id = mgr.start_jobs("tool-hang", &[src]).unwrap().remove(0);
        let deadline = Instant::now() + Duration::from_secs(30);
        while !engines.in_use("tool") {
            assert!(Instant::now() < deadline, "the job never took a lease");
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(engines.remove("tool").unwrap_err(), ENGINE_BUSY);
        mgr.cancel(&id);
        assert_eq!(events.wait_end(&id).status, "cancelled");
        assert!(!engines.in_use("tool"), "the lease ends with the job");
        engines.remove("tool").unwrap();
    }

    #[test]
    fn long_destination_paths_use_the_scratch_temp_dir() {
        let scratch = Path::new(r"C:\Users\x\AppData\Local\Recast\tmp");
        let short = Path::new(r"C:\d");
        assert_eq!(tmp_dir_for(short, "j1", scratch).unwrap(), Path::new(r"C:\d\.recast-j1"));
        let long = PathBuf::from(format!(r"C:\{}", "x".repeat(250)));
        assert_eq!(tmp_dir_for(&long, "j1", scratch).unwrap(), scratch.join(".recast-j1"));
        let other_drive = PathBuf::from(format!(r"D:\{}", "x".repeat(250)));
        assert_eq!(tmp_dir_for(&other_drive, "j1", scratch).unwrap_err().error.code, ErrorCode::DestUnwritable);
    }
}
