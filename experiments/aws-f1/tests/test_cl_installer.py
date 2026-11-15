"""Regression against real pinned F1 cl_hello_world template.

No FPGA, license or AWS SDK needed; output belongs to an automatically cleaned
temporary folder below the conversation's managed build directory.
"""
from __future__ import annotations

import os
import pathlib
import subprocess
import sys
import tempfile


ROOT = pathlib.Path(__file__).resolve().parents[1]
EXAMPLE = pathlib.Path(
    os.environ.get(
        "AWS_F1_HELLO_WORLD",
        "/home/jan/references/aws-fpga-f1-v1.4.25/hdk/cl/examples/cl_hello_world",
    )
)
OUT = pathlib.Path("/home/jan/builds/hashwx-f1-20261115")
FILES = (
    "hashwx_core.sv",
    "hashwx_scan.sv",
    "hashwx_mmio.sv",
    "hashx_core.sv",
    "hashx_scan.sv",
    "hashx_mmio.sv",
    "hash_parallel_scan.sv",
    "hash_job_queue.sv",
    "hash_pcim_dma.sv",
    "hash_f1_dual.sv",
)


def main() -> None:
    if not (EXAMPLE / "design" / "cl_hello_world.sv").is_file():
        raise SystemExit(
            "F1 HDK v1.4.25 cl_hello_world required for template regression; "
            "set AWS_F1_HELLO_WORLD to its directory"
        )
    OUT.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="cl-check-", dir=OUT) as temp:
        target = pathlib.Path(temp) / "cl_dual"
        command = [
            sys.executable,
            str(ROOT / "scripts" / "install-f1-cl.py"),
            "--example",
            str(EXAMPLE),
            "--dest",
            str(target),
        ]
        subprocess.run(command, check=True)
        top = (target / "design" / "cl_hello_world.sv").read_text()
        source = (EXAMPLE / "design" / "cl_hello_world.sv").read_text()
        encrypted = (target / "build" / "scripts" / "encrypt.tcl").read_text()
        assert top.count("hash_f1_dual HASH_ENGINES") == 1
        assert top.count("hash_read_data") == 3
        assert chr(96) + "UNIMPLEMENTED_REG_VALUE" not in top
        assert chr(96) + "UNIMPLEMENTED_REG_VALUE" in source
        assert "HASH_ENGINES" not in source
        assert 'unused_pcim_template.inc' not in top
        assert 'unused_pcim_template.inc' in source
        assert '.pcim_araddr(cl_sh_pcim_araddr)' in top
        assert '.pcim_wdata(cl_sh_pcim_wdata)' in top
        for name in FILES:
            assert (target / "design" / name).read_bytes() == (
                ROOT / "rtl" / name
            ).read_bytes()
            assert encrypted.count("file copy -force $CL_DIR/design/" + name) == 1
        repeated = subprocess.run(command, capture_output=True, text=True)
        assert repeated.returncode != 0
        assert "refusing overwrite" in repeated.stderr + repeated.stdout
    print("PASS real AWS F1 v1.4.25 CL generator, ten modules, no source mutation")


if __name__ == "__main__":
    main()
