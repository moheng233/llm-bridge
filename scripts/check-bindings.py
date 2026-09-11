#!/usr/bin/env python3
"""在临时目录导出 Rust 类型，比较绑定而不改写仓库。"""
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
BINDINGS = ROOT / "frontend/src/bindings"


def main():
    with tempfile.TemporaryDirectory(prefix="llm-bridge-bindings-") as directory:
        generated = Path(directory)
        environment = dict(os.environ, TS_RS_EXPORT_DIR=directory)
        subprocess.run(
            ["cargo", "test", "--lib", "--locked", "export_bindings"],
            cwd=ROOT, env=environment, check=True,
        )
        expected = {path.relative_to(generated) for path in generated.rglob("*.ts")}
        committed = {
            path.relative_to(BINDINGS) for path in BINDINGS.rglob("*.ts")
            if path != BINDINGS / "client.ts"
        }
        problems = []
        for path in sorted(expected | committed):
            if path not in expected:
                problems.append(f"obsolete: {path}")
            elif path not in committed:
                problems.append(f"missing: {path}")
            elif (generated / path).read_bytes() != (BINDINGS / path).read_bytes():
                problems.append(f"changed: {path}")
        if problems:
            raise SystemExit("TypeScript binding drift:\n" + "\n".join(problems))
    subprocess.run(
        ["cargo", "test", "--locked", "--test", "generate_ts_client", "check_ts_client_up_to_date", "--", "--exact"],
        cwd=ROOT, check=True,
    )
    print("Rust type bindings and API client match committed files.")


if __name__ == "__main__":
    main()
