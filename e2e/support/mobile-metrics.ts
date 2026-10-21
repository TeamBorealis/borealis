/**
 * m4 (S5 mobile acceptance) numbers as the `METRIC<TAB>source<TAB>stage<TAB>*<TAB>metric<TAB>n` lines that
 * tools/ci/check_thresholds.py reads, so one checker and one tools/acceptance/thresholds.json judge desktop
 * and mobile runs. Keys match the android/ios rows of the thresholds file. No imports: plain data in, lines out.
 */

/** m4 time (ms) name -> `source/stage` key of the thresholds file. */
export const M4_STAGES: Record<string, string> = {
  aCoreMs: 'a-core/import',
  aUserMs: 'a-user/import',
  fFullMs: 'f-full/import',
  restartToDigestMs: 'app/restart_digest',
};

/** Key of the whole-run peak memory row. */
export const M4_PROCESS_KEY = 'process/m4';

const row = (key: string, metric: string, value: number) => `METRIC\t${key.replace('/', '\t')}\t*\t${metric}\t${value}`;

/** wall_s per stage (rounded seconds) and peak_mem_mb (MiB, the highest sample over the run). */
export function m4MetricLines(times: Record<string, number>, peakKb: Record<string, number | undefined>): string[] {
  const lines = Object.entries(M4_STAGES)
    .filter(([name]) => times[name] !== undefined)
    .map(([name, key]) => row(key, 'wall_s', Math.round(times[name] / 1000)));
  const peaks = Object.values(peakKb).filter((kb): kb is number => kb !== undefined);
  if (peaks.length) lines.push(row(M4_PROCESS_KEY, 'peak_mem_mb', Math.round(Math.max(...peaks) / 1024)));
  return lines;
}
