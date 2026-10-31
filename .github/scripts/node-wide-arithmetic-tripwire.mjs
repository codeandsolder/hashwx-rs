// Fails intentionally once Node/V8 accepts WebAssembly wide arithmetic by default.
//
// This is a minimal valid module containing `i64.mul_wide_u`. Wasmtime 50
// accepts these exact bytes with `-Wwide-arithmetic=y`; current Node rejects
// opcode 0xfc16. When this starts failing, revisit HashX's portable limb-based
// mul-high lowering and switch the default emitter once runtime coverage allows.

const wideArithmeticProbe = new Uint8Array([
  0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00,
  0x01, 0x08, 0x01, 0x60, 0x02, 0x7e, 0x7e, 0x02, 0x7e, 0x7e,
  0x03, 0x02, 0x01, 0x00,
  0x07, 0x05, 0x01, 0x01, 0x66, 0x00, 0x00,
  0x0a, 0x0a, 0x01, 0x08, 0x00, 0x20, 0x00, 0x20, 0x01, 0xfc, 0x16, 0x0b,
]);

if (WebAssembly.validate(wideArithmeticProbe)) {
  console.error(
    `Node ${process.version} now accepts WebAssembly wide arithmetic; ` +
      'replace HashX Wasm mul-high limb lowering with i64.mul_wide_{u,s} and update this tripwire.',
  );
  process.exit(1);
}

console.log(
  `Node ${process.version} still rejects WebAssembly wide arithmetic (expected); keeping portable HashX mul-high lowering.`,
);
