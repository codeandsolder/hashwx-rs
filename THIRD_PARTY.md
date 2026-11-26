# Third-party provenance

`hashwx` combines original Rust backend work with compatibility implementations derived from upstream LGPL code.

## HashWX

The HashWX algorithm, program model, and compatibility behavior are based on tevador's `hashwx` reference implementation:

- Source: <https://github.com/tevador/hashx/tree/master/hashwx>
- Upstream license: LGPL-3.0-or-later

The Rust implementation and native/WebAssembly backends in this repository are independently adapted around that behavior.

## HashX

The in-tree HashX v1 compatibility model is adapted from the Tor Project's Arti `hashx` crate, version 0.10.0:

- Authors: The Tor Project, Inc.; Micah Elizabeth Scott
- Source: <https://gitlab.torproject.org/tpo/core/arti.git/>
- Crate: `hashx` 0.10.0
- Upstream license: LGPL-3.0-only

The `hashx` crate is also used as a development-only differential oracle; it is not a normal dependency of the published library.

Because the in-tree HashX-derived code is LGPL-3.0-only, this combined crate is distributed under LGPL-3.0-only. See `LICENSE` for the license text.

## License grant for original contributions

Code and Solder's original contributions are additionally available under
MIT OR Apache-2.0, at your option (see `LICENSE-MIT` and `LICENSE-APACHE`).
This grant covers original work only, not upstream-derived expression or
third-party contributions, which retain their original licensing. In particular,
the HashX compatibility model under `src/hashx/` and HashWX-derived tables
in `src/tables.rs` retain their LGPL terms. The combined crate continues
to be distributed under LGPL-3.0-only; this grant does not relicense it as a whole.
