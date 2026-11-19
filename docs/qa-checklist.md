# Recast manual QA checklist

Run this before each release, against a **release install** (`Recast_0.1.0_x64-setup.exe`), not `npm run tauri dev`. A dev build has no app identity for notifications and doesn't write the Explorer verb or the Run value unless `RECAST_DEV_INTEGRATION=1`. This is spec §9's manual row, written out.

Each item says what to do and what should happen. Tick it when it does. Note the Windows version, the DPI scale and the monitor setup at the top of your run.

Run: ______  Windows build: ______  Scale / monitors: ______

## Install and upgrade

- [ ] Install the setup exe on a clean profile: it needs no admin prompt, installs per user and starts Recast. The first-run welcome appears once.
- [ ] Install a newer build **over** an existing install (the reinstall page, no uninstall first). Your downloaded engines (Library shows them Installed) and your Activity history are still there afterwards.

## Triggers

- [ ] **Edge handle drag:** drag a file from Explorer onto the left-edge tab. The pad opens beside the tab (not on top of it), and you can release on a segment to convert.
- [ ] **Drop on the handle:** drop a file on the tab itself. The pad opens in click mode with the file, takes focus, and a click on a segment converts.
- [ ] **Auto-hide over fullscreen:** play a video fullscreen in a browser or player on the primary monitor. The handle disappears. Leave fullscreen and it returns within a second. A maximized (not fullscreen) window does not hide it.
- [ ] **Edge and position settings:** in Settings, change the edge (left, right, top, bottom) and the position along the edge. The tab moves at once to the new place. Turn the handle off: it disappears. Turn it on: it returns.
- [ ] **Desk pad drag and drop:** turn the desk pad on. A round pad appears at the bottom right of the primary monitor. Drag a file onto it: the pad opens over it. Drop on it: the pad opens in click mode.
- [ ] **Desk pad position remembered:** drag the desk pad somewhere else, quit Recast from the tray, start it again. The pad is where you left it. After a restart without moving it, it is at the default spot and nothing is saved.
- [ ] **Win+D:** with the desk pad on, press Win+D. The desk pad goes away with the other windows (it sits at desktop level) and returns when you show the windows again.
- [ ] **Shake from Explorer:** hold a file from Explorer and shake it left and right (three quick reversals). The pad opens; release on a segment to convert.
- [ ] **Shake probe on real hardware:** with shake on, drag a real Explorer file and shake it. The log (`%LOCALAPPDATA%\Recast\logs\recast.log`) shows `shake: armed` and then `pad: drag entered`, and the pad becomes visible. If it stays invisible, note it: the planned fix is a near-transparent `.pad-body` background while probing.
- [ ] **Shake false positives:** each of these must NOT open the pad: shaking while selecting text in a document; painting with a brush in an image editor; shaking in a fullscreen game; Aero Shake (shaking a window by its title bar); shaking the mouse with no button down.
- [ ] **Shake off while fullscreen:** with a fullscreen app on any monitor, a file shake opens nothing.
- [ ] **Auto-close after a shake:** shake a file, then release it in empty space (no drop on the pad). About 1.5 s after the button goes up, the pad closes by itself. The same happens for a pad opened by dragging onto the handle and then releasing elsewhere.
- [ ] **Auto-hide taskbar:** set the taskbar to auto-hide, with the handle on the taskbar's edge. The handle stands 2 px off the edge and the taskbar still reveals when you push the cursor to the edge.
- [ ] **Taskbar moved:** move the taskbar to another edge, or resize it. The handle and desk pad re-place within a second and stay inside the work area.
- [ ] **Display, DPI or monitor-layout change:** change the scale (100% to 150%), plug or unplug a monitor, or change which monitor is primary. The handle and desk pad move to sensible places on the primary work area (the desk pad stays on its monitor if that monitor remains).

## Explorer entry and start with Windows

- [ ] **Open with Recast, 1 file:** right-click a file, Show more options, Open with Recast. One pad opens at the cursor with the file and takes keyboard focus (arrow keys move around the ring, Enter converts, Esc closes).
- [ ] **20 files and 100 files:** select 20, then 100 files, and use the same verb. Exactly one pad opens, with all the files ("20 files").
- [ ] **Entry removed when off:** turn the Explorer entry off in Settings. Right-click a file: the verb is gone. Turn it on: it returns.
- [ ] **Start with Windows:** it is on by default. Sign out and in. Recast starts in the tray with no window. Turn it off in Settings: after the next sign-in Recast does not start.
- [ ] **First-run welcome once:** on a fresh profile the companion opens with the three-step welcome and the handle glows with "Drag a file here" while it is in front. Finish or skip it. Restart Recast: the welcome does not return.

## Pad

- [ ] **DPI:** at 100%, 150% and 200%, the pad is sharp, the ring is centred on the cursor or anchor, and nothing is clipped by the screen edge.
- [ ] **Two monitors:** open the pad from a trigger on each monitor. It appears on the monitor of the cursor or widget, inside the work area.
- [ ] **Reduced motion and reduced transparency:** turn off Windows animations: the ring opens without the materialize animation. Turn off transparency effects: the ring uses the solid look.
- [ ] **Keyboard only:** with the pad open, use Tab or the arrows to move, Enter to run, Esc to close, and Enter on More and Back to change page.
- [ ] **Light and dark wallpapers:** segments over light areas flip to dark symbols; over dark areas they stay white.
- [ ] **No ghost ring:** the glass of a segment never shows the pad's own ring.
- [ ] **Frame time:** open `pad.html?perf` in a dev build and read `[pad] mean draw ... ms` in the console while hovering. It is at most 4 ms, also on integrated graphics.
- [ ] **Real Explorer drag:** drag a file from Explorer onto the pad (opened by the handle). Targeting follows the pointer and a drop on a segment starts the conversion.

## Notifications (release install)

- [ ] **Show in folder:** convert a file. The "ready" toast has Show in folder; clicking it opens Explorer with the output selected.
- [ ] **Details:** make a conversion fail (a corrupt file). The toast has Details; clicking it opens Activity with that job expanded.
- [ ] **Save to fallback folder:** convert into a read-only folder. The toast offers Save to fallback folder and Details; the first re-runs the job into `Documents\Recast`.
- [ ] **Locked output:** make the output file locked at rename (for example held open by another program). After about 5 s the toast offers Retry and Details.
- [ ] **Engine Retry:** disconnect the network during a first engine download. The failure toast has Retry. Reconnect and click it: the engine installs. Click Retry while a conversion is using that engine: you get an "in use" toast.
- [ ] **Clicks while the toast is on screen:** buttons work only while the toast is visible; clicking the toast body opens Activity (Library for an engine failure).
- [ ] **No toast while the window is in front:** with the companion visible and focused, job notifications are suppressed.

## Mixed kinds

- [ ] **Pad Open segment:** drop a video and an image together on a trigger. The ring has one segment, Open, with "Open in Recast · convert each kind on its own" in its pill. Activating it opens Activity with the files in groups.
- [ ] **Activity groups:** each group (for example "1 video", "1 image") has Choose conversion, which opens the ring for that group only. Dropping a mixed set on the companion shows the groups directly.

## Tools

- [ ] **Reorder by mouse and keyboard:** in Merge PDFs, drag a row by its handle; then focus a handle, press Space, use the arrow keys and press Space again.
- [ ] **Merge:** merge three PDFs in a changed order; the output follows that order.
- [ ] **Split with valid and invalid ranges:** `1-3, 5, 8-` makes `pages-1-3.pdf`, `page-5.pdf`, `pages-8-end.pdf`. `3-1`, `0` and `abc` show an error in the field and start nothing.

## Tray

- [ ] **Convert files…:** the tray menu is Open Recast, Convert files… and Quit (no Drop pad). Convert files… opens a picker and then the pad at the cursor.
- [ ] **Glyph follows the taskbar theme:** switch the taskbar between light and dark (Settings, Personalization, Colors, "Choose your default Windows mode"). The tray glyph changes within about a second to the matching monochrome one.

## Uninstall

- [ ] **Plain uninstall:** uninstall with "Delete app data" unticked. The Explorer verb is gone (`HKCU\Software\Classes\*\shell\Recast`), the Run value is gone (`HKCU\Software\Microsoft\Windows\CurrentVersion\Run\Recast`), and `%LOCALAPPDATA%\Recast\logs` is gone. `%LOCALAPPDATA%\Recast` (engines, `recast.db`) and `%APPDATA%\Recast` (settings) are still there.
- [ ] **Uninstall with Delete app data ticked:** also `%LOCALAPPDATA%\Recast` and `%APPDATA%\Recast` are gone.

## Elevated apps

- [ ] **Elevated drag ignored:** drag a file out of an app running as administrator onto the handle. Nothing happens and Recast logs no error. (Windows blocks it; the Settings help text says so.)

## Plan 1 manual checks

- [ ] A real `.mov` drop through download, convert, notification and Show in folder.
- [ ] Three PNGs give one combined notification.
- [ ] A mixed-kinds drop.
- [ ] Cancel leaves no `.recast-*` folder.
- [ ] Library verify, remove and re-download.
- [ ] A settings change persists across a restart.
- [ ] Tray hide and reopen.
- [ ] Quitting mid-conversion, then relaunching: no temp folders remain and the job shows as interrupted.
