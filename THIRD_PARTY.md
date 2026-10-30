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
