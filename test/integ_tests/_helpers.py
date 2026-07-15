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

"""Shared helpers for integration tests."""

from __future__ import annotations

from scipy.stats import chi2_contingency

SHOTS = 2000
TOLERANCE = 0.10
CHI2_SIGNIFICANCE = 0.001


def approx(actual: float, expected: float, tol: float = TOLERANCE) -> bool:
    return abs(actual - expected) <= tol


def assert_distributions_equivalent(
    counts_a: dict[str, int],
    counts_b: dict[str, int],
    significance: float = CHI2_SIGNIFICANCE,
) -> None:
    """Two-sample chi-squared test; fails only if p < significance."""
    all_outcomes = sorted(set(counts_a) | set(counts_b))
    filtered = [(counts_a.get(k, 0), counts_b.get(k, 0)) for k in all_outcomes]
    filtered = [(a, b) for a, b in filtered if a + b > 0]
    if not filtered:
        return
    row_a, row_b = zip(*filtered, strict=True)
    chi2_stat, p_value, _, _ = chi2_contingency([list(row_a), list(row_b)])
    assert p_value >= significance, (
        f"Distributions differ significantly: chi2={chi2_stat:.2f}, "
        f"p={p_value:.6f} < {significance}\n"
        f"  counts_a: {counts_a}\n  counts_b: {counts_b}"
    )
