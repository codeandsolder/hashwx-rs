#!/usr/bin/env python3
"""Generate a self-contained F1 CL from AWS HDK v1.4.25 cl_hello_world.

Keeps AWS originals and notices; never modifies the reference checkout.
"""
import argparse
import shutil
from pathlib import Path

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--example", type=Path, required=True, help="AWS v1.4.25 hdk/cl/examples/cl_hello_world")
    parser.add_argument("--dest", type=Path, required=True, help="new cl_hashwx directory (must not exist)")
    args = parser.parse_args()
    example = args.example.resolve(strict=True)
    dest = args.dest.resolve()
    here = Path(__file__).resolve().parent.parent
    assert (example / "design" / "cl_hello_world.sv").is_file()
    if dest.exists():
        raise SystemExit(f"destination exists; refusing overwrite: {dest}")
    shutil.copytree(example, dest, symlinks=True, ignore=shutil.ignore_patterns(".git", "build.log", "*.dcp", "*.tar"))
    rtl_modules = (
        "hashwx_core.sv", "hashwx_scan.sv", "hashwx_mmio.sv",
        "hashx_core.sv", "hashx_scan.sv", "hashx_mmio.sv",
    )
    for filename in rtl_modules:
        shutil.copy2(here / "rtl" / filename, dest / "design" / filename)
    top = dest / "design" / "cl_hello_world.sv"
    source = top.read_text()
    needle = "// Read Response\nalways_ff"
    insert = """// HashWX engine register bank, behind the standard F1 AXI-Lite OCL.
logic [31:0] hashwx_read_data;
hashwx_mmio HASHWX_MMIO (
    .clk(clk_main_a0),
    .rst(!rst_main_n_sync),
    .wr_en(wready && (wstrb == 4'hf)),
    .wr_addr(wr_addr), .wr_data(wdata),
    .rd_addr(araddr_q), .rd_value(hashwx_read_data)
);
logic [31:0] hashx_read_data;
hashx_mmio HASHX_MMIO (
    .clk(clk_main_a0),
    .rst(!rst_main_n_sync),
    .wr_en(wready && (wstrb == 4'hf)),
    .wr_addr(wr_addr), .wr_data(wdata),
    .rd_addr(araddr_q), .rd_value(hashx_read_data)
);

// Read Response
always_ff"""
    assert source.count(needle) == 1
    source = source.replace(needle, insert)
    needle = chr(96) + "UNIMPLEMENTED_REG_VALUE        ;"
    assert source.count(needle) == 1
    source = source.replace(
        needle,
        "((araddr_q >= 32'h1000 && araddr_q < 32'h1050)"
        " ? hashx_read_data : hashwx_read_data);",
    )
    top.write_text(source)
    encrypt = dest / "build" / "scripts" / "encrypt.tcl"
    text = encrypt.read_text()
    marker = "#---- End of section replaced by Developr ---"
    assert text.count(marker) == 1
    added = "".join(
        f"file copy -force $CL_DIR/design/{name} $TARGET_DIR\n"
        for name in rtl_modules
    )
    encrypt.write_text(text.replace(marker, added + marker))
    print(f"Generated {dest}; F1 top and encrypt.tcl updated, {len(rtl_modules)} RTL files installed.")

if __name__ == "__main__":
    main()
