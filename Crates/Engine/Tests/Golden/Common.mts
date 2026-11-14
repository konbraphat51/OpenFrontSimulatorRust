// SPDX-License-Identifier: AGPL-3.0-only
// Shared helpers of the golden generators: upstream module loading and exact f64 encoding.
// Run a generator from the baseline worktree:
//   cd <worktree> && OPENFRONT_WORKTREE=<worktree> npx tsx <this dir>/XGolden.mts > <this dir>/X.json
const f64 = new Float64Array(1);
const u64 = new BigUint64Array(f64.buffer);

/** Encodes a number as its 16-digit hex IEEE 754 bit pattern. */
export function bits(x: number): string {
  f64[0] = x;
  return u64[0].toString(16).padStart(16, "0");
}

/** Imports an upstream module from `src/` of the baseline worktree. */
export async function upstream(path: string): Promise<any> {
  const root = process.env.OPENFRONT_WORKTREE;
  if (!root) throw new Error("set OPENFRONT_WORKTREE");
  return import(`${root}/src/${path}`);
}
