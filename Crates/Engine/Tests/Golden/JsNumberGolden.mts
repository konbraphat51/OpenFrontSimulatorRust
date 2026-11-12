// Generates JsNumber.json: ToInt32, ToUint32, Math.round, toString(36) and padStart computed by node itself.
import { bits } from "./Common.mts";

const xs = [
  0, -0, 0.5, -0.5, 1.5, 2.5, -2.5, -0.4, 0.49999999999999994, 1e20, -1e20,
  2147483647, 2147483648, -2147483648, -2147483649, 4294967295, 4294967296,
  4294967297.5, 9007199254740991, 4503599627370495.5, 4503599627370497,
  1e300, Infinity, -Infinity, NaN, 123456.789, -123456.789, 3.999999999999999,
];
for (let i = 0; i < 200; i++) xs.push((i - 100) * 2654435.761 + 0.5);
const radix = [0, 1, 35, 36, 1295, 1296, 123456789, 2821109907455, 9007199254740991];
console.log(
  JSON.stringify({
    int32: xs.map((x) => [bits(x), x | 0, x >>> 0, bits(Math.round(x))]),
    radix: radix.map((n) => [n, n.toString(36)]),
    pad: [["ab", 8], ["", 3], ["abcdefghij", 8]].map(([s, n]: any) => [s, n, s.padStart(n, "0")]),
  }),
);
