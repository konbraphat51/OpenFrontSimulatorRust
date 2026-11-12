// SPDX-License-Identifier: AGPL-3.0-only
// Generates V8Math.json: for each Math function, [input bits..., output bits] as hex strings. Run: node V8Math.mjs > V8Math.json
const buf = new DataView(new ArrayBuffer(8));
const bits = (x) => { buf.setFloat64(0, x); return buf.getBigUint64(0).toString(16).padStart(16, "0"); };
const fromBits = (b) => { buf.setBigUint64(0, b); return buf.getFloat64(0); };
let seed = 0x12345678n;
const rnd64 = () => { seed ^= seed << 13n; seed &= 0xffffffffffffffffn; seed ^= seed >> 7n; seed ^= seed << 17n; seed &= 0xffffffffffffffffn; return seed; };
const rnd = () => Number(rnd64() >> 11n) / 2 ** 53;
// Random double with exponent uniformly in [lo, hi] and random sign.
const expRand = (lo, hi) => (rnd() < 0.5 ? -1 : 1) * (1 + rnd()) * 2 ** Math.floor(lo + rnd() * (hi - lo + 1));
const anyBits = () => fromBits(rnd64());
const ulp = (x, k) => fromBits(BigInt.asUintN(64, buf.setFloat64(0, x) ?? buf.getBigUint64(0) + BigInt(k)));
const special = [0, -0, Infinity, -Infinity, NaN, 5e-324, -5e-324, 2.2250738585072014e-308, 1.7976931348623157e308, -1.7976931348623157e308,
  1, -1, 0.5, -0.5, 2, -2, 3, 10, 100, Math.PI, Math.PI / 2, Math.PI / 4, -Math.PI, 2 * Math.PI, 1e22, 1e300, 710, 709.78, -745.2, -744, 22, 1e-10, 0.3, 0.7, 0.78125, 1.5, 0.1, 0.9999999999999999, 1.0000000000000002, 1e5, 1e6, 1e15, 2 ** 53, 2 ** 20 * Math.PI / 2, 2 ** 30, 2 ** 1000, 2 ** -1000, 7, 1 / 3];
const unary = (gen) => { const a = [...special, ...special.map((s) => ulp(s, 1)), ...special.map((s) => ulp(s, -1))]; for (let i = 0; i < 1200; i++) a.push(gen(i)); return a; };
const gens = {
  default: (i) => (i % 3 === 0 ? expRand(-40, 40) : i % 3 === 1 ? anyBits() : (rnd() * 2 - 1) * 10),
  unit: (i) => (i % 2 === 0 ? rnd() * 2 - 1 : (rnd() < 0.5 ? -1 : 1) * 2 ** (-Math.floor(rnd() * 60)) * rnd()),
  trig: (i) => (i % 4 === 0 ? expRand(-30, 10) : i % 4 === 1 ? expRand(10, 1000) : i % 4 === 2 ? (rnd() * 2 - 1) * 20 : anyBits()),
  pos: (i) => (i % 2 === 0 ? expRand(-1070, 1000) : Math.abs(anyBits())),
  exp: (i) => (i % 2 === 0 ? (rnd() * 2 - 1) * 750 : expRand(-60, 10)),
  hyper: (i) => (i % 2 === 0 ? (rnd() * 2 - 1) * 720 : expRand(-40, 8)),
};
const out = {};
const un = (name, fn, g) => { out[name] = unary(gens[g]).map((x) => [bits(x), bits(fn(x))]); };
un("exp", Math.exp, "exp"); un("expm1", Math.expm1, "exp");
un("log", Math.log, "pos"); un("log2", Math.log2, "pos"); un("log10", Math.log10, "pos"); un("log1p", Math.log1p, "default");
un("atan", Math.atan, "default"); un("sin", Math.sin, "trig"); un("cos", Math.cos, "trig"); un("tan", Math.tan, "trig");
un("asin", Math.asin, "unit"); un("acos", Math.acos, "unit");
un("sinh", Math.sinh, "hyper"); un("cosh", Math.cosh, "hyper"); un("tanh", Math.tanh, "hyper");
un("cbrt", Math.cbrt, "default");
const pairs = (fn, genPair) => {
  const a = [];
  for (const x of special) for (const y of special) a.push([x, y]);
  for (let i = 0; i < 1200; i++) a.push(genPair(i));
  return a.map(([x, y]) => [bits(x), bits(y), bits(fn(x, y))]);
};
out.atan2 = pairs(Math.atan2, (i) => (i % 2 ? [expRand(-40, 40), expRand(-40, 40)] : [anyBits(), anyBits()]));
out.pow = pairs(Math.pow, (i) => {
  switch (i % 6) {
    case 0: return [expRand(-20, 20), expRand(-5, 5)];
    case 1: return [-Math.abs(expRand(-5, 5)), Math.floor((rnd() * 2 - 1) * 40)];
    case 2: return [Math.abs(expRand(-10, 10)), 0.5 * Math.floor((rnd() * 2 - 1) * 20)];
    case 3: return [anyBits(), anyBits()];
    case 4: return [1 + (rnd() * 2 - 1) * 1e-9, expRand(20, 60)];
    default: return [Math.floor(rnd() * 100), Math.floor(rnd() * 40)];
  }
});
out.hypot = pairs(Math.hypot, (i) => [expRand(-500, 500), expRand(-500, 500)]);
console.log(JSON.stringify(out));
