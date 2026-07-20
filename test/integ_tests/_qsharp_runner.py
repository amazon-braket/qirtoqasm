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

"""Subprocess runner: python _qsharp_runner.py <fixture> <entry> <profile> <shots> [--native-sim]"""

from __future__ import annotations

import json
import sys
from collections import Counter
from pathlib import Path

import qirtoqasm
import qsharp
from braket.devices import LocalSimulator
from braket.ir.openqasm import Program


def native_sim(source: str, entry: str, shots: int) -> dict:
    qsharp.init(target_profile=qsharp.TargetProfile.Unrestricted)
    qsharp.eval(source)
    raw_results = qsharp.run(entry, shots=shots)
    counts: Counter = Counter()
    for r in raw_results:
        if isinstance(r, (tuple, list)):
            bits = "".join("1" if str(b) == "One" else "0" for b in r)
        else:
            bits = "1" if str(r) == "One" else "0"
        counts[bits] += 1
    return dict(counts)


def braket_pipeline(source: str, entry: str, profile: str, shots: int) -> tuple[dict, str]:
    qsharp.init(target_profile=getattr(qsharp.TargetProfile, profile))
    qsharp.eval(source)
    qir_text = str(qsharp.compile(entry))

    openqasm = qirtoqasm.translate(qir_text)
    result = LocalSimulator().run(Program(source=openqasm), shots=shots).result()
    return dict(result.measurement_counts), openqasm


def main():
    fixture_path = sys.argv[1]
    entry = sys.argv[2]
    profile = sys.argv[3]
    shots = int(sys.argv[4])
    do_native = "--native-sim" in sys.argv

    source = Path(fixture_path).read_text()

    payload: dict = {}
    if do_native:
        payload["native_counts"] = native_sim(source, entry, shots)

    braket_counts, openqasm = braket_pipeline(source, entry, profile, shots)
    payload["counts"] = braket_counts
    payload["openqasm"] = openqasm

    print(json.dumps(payload))


if __name__ == "__main__":
    main()
