/** The inline ring reports "started" once the jobs really run (start_jobs resolved), never on a start that fails. */
export function shouldReportStarted(phaseName: string, alreadyReported: boolean): boolean {
  return phaseName === "running" && !alreadyReported;
}
