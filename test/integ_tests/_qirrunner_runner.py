# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
#
# Licensed under the Apache License, Version 2.0 (the "License"). You
# may not use this file except in compliance with the License. A copy of
# the License is located at
#
#     http://aws.amazon.com/apache2.0/
#
# or in the "license" file accompanying this file. This file is
# distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF
# ANY KIND, either express or implied. See the License for the specific
# language governing permissions and limitations under the License.

"""Subprocess runner: python _qirrunner_runner.py <ll_path> <shots>

Runs a .ll file through qirrunner, parses OUTPUT lines into measurement
counts, and prints JSON to stdout.
"""

from __future__ import annotations

import json
import os
import sys
import tempfile
from collections import Counter
from pathlib import Path

import qirrunner


def main():
    ll_path = sys.argv[1]
    shots = int(sys.argv[2])

    # Strip target triple to avoid cross-architecture JIT segfaults
    # (e.g., x86_64 triple on ARM host).
    tmp_path = None
    content = Path(ll_path).read_text()
    if "target triple" in content:
        ir_lines = [
            line
            for line in content.splitlines(keepends=True)
            if not line.startswith("target triple")
        ]
        fd, tmp_path = tempfile.mkstemp(suffix=".ll")
        with os.fdopen(fd, "w") as f:
            f.writelines(ir_lines)
        ll_path = tmp_path

    try:
        raw_outputs: list[str] = []
        qirrunner.run(ll_path, shots=shots, output_fn=lambda o: raw_outputs.append(str(o)))
    finally:
        if tmp_path is not None:
            os.unlink(tmp_path)

    output_lines: list[str] = []
    for chunk in raw_outputs:
        output_lines.extend(chunk.splitlines())

    counts: Counter = Counter()
    shot_bits: list[str] = []
    for line in output_lines:
        if line.startswith("OUTPUT\tRESULT\t"):
            shot_bits.append(line.split("\t")[2])
        elif line.startswith("END\t"):
            if shot_bits:
                counts["".join(shot_bits)] += 1
            shot_bits = []

    print(json.dumps(dict(counts)))


if __name__ == "__main__":
    main()
