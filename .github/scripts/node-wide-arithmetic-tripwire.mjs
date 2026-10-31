// Fails intentionally once Node/V8 accepts WebAssembly wide arithmetic by default.
//
// These are minimal valid modules containing `i64.mul_wide_s` / `i64.mul_wide_u`.
// Wasmtime 50 accepts the exact bytes with `-Wwide-arithmetic=y`; current Node
// rejects opcodes 0xfc15/0xfc16. When both start validating, replace HashX's
// portable limb-based mul-high lowering once runtime coverage allows.

function wideArithmeticProbe(opcode) {
  return new Uint8Array([
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00,
    0x01, 0x08, 0x01, 0x60, 0x02, 0x7e, 0x7e, 0x02, 0x7e, 0x7e,
    0x03, 0x02, 0x01, 0x00,
    0x07, 0x05, 0x01, 0x01, 0x66, 0x00, 0x00,
    0x0a, 0x0a, 0x01, 0x08, 0x00, 0x20, 0x00, 0x20, 0x01, 0xfc, opcode, 0x0b,
  ]);
}

const signed = WebAssembly.validate(wideArithmeticProbe(0x15));
const unsigned = WebAssembly.validate(wideArithmeticProbe(0x16));

if (signed && unsigned) {
  console.error(
    `Node ${process.version} now accepts WebAssembly wide arithmetic; ` +
      'replace HashX Wasm mul-high limb lowering with i64.mul_wide_{u,s} and update this tripwire.',
  );
  process.exit(1);
}

console.log(
  `Node ${process.version} wide arithmetic: signed=${signed} unsigned=${unsigned} (both required; keeping portable HashX mul-high lowering).`,
);
