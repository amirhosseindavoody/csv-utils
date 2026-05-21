#!/usr/bin/env python3
"""
Generate CSV performance test datasets with mixed column types.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass
from pathlib import Path
import string
from typing import Iterable

import numpy as np
import pandas as pd


ALPHABET = np.array(list(string.ascii_letters + string.digits), dtype="<U1")


@dataclass(frozen=True)
class DatasetSpec:
    rows: int
    cols: int

    @property
    def name(self) -> str:
        return f"{self.rows}x{self.cols}"

    @property
    def filename(self) -> str:
        return f"test_{self.rows}x{self.cols}.csv"


@dataclass(frozen=True)
class ColumnLayout:
    str_cols: int
    long_str_cols: int
    float_general_cols: int
    float_scientific_cols: int
    float_mixed_cols: int
    int_cols: int
    date_cols: int

    def total(self) -> int:
        return (
            self.str_cols
            + self.long_str_cols
            + self.float_general_cols
            + self.float_scientific_cols
            + self.float_mixed_cols
            + self.int_cols
            + self.date_cols
        )


SPECS = {
    "1000x100": DatasetSpec(1_000, 100),
    "10000x1000": DatasetSpec(10_000, 1_000),
}

DATASET_SEED_OFFSETS = {
    "1000x100": 101,
    "10000x1000": 202,
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Generate CSV benchmark datasets.")
    parser.add_argument(
        "--datasets",
        nargs="+",
        choices=list(SPECS.keys()),
        default=list(SPECS.keys()),
        help="Dataset sizes to generate.",
    )
    parser.add_argument(
        "--output-dir",
        default="test-data/generated",
        help="Output directory for generated CSV files.",
    )
    parser.add_argument(
        "--chunk-rows",
        type=int,
        default=20_000,
        help="Rows to generate per chunk for large files.",
    )
    parser.add_argument(
        "--seed",
        type=int,
        default=42,
        help="Random seed for deterministic data generation.",
    )
    return parser.parse_args()


COLUMN_TYPE_COUNT = 7


def build_layout(total_cols: int) -> ColumnLayout:
    if total_cols < COLUMN_TYPE_COUNT:
        counts = [0] * COLUMN_TYPE_COUNT
        for i in range(total_cols):
            counts[i] = 1
        return ColumnLayout(
            str_cols=counts[0],
            long_str_cols=counts[1],
            float_general_cols=counts[2],
            float_scientific_cols=counts[3],
            float_mixed_cols=counts[4],
            int_cols=counts[5],
            date_cols=counts[6],
        )

    remaining = total_cols - COLUMN_TYPE_COUNT
    long_str_cols = 1 + max(1, int(remaining * 0.03))
    float_general_cols = 1 + max(3, int(remaining * 0.15))
    float_scientific_cols = 1 + max(3, int(remaining * 0.10))
    float_mixed_cols = 1 + max(3, int(remaining * 0.05))
    int_cols = 1 + max(7, int(remaining * 0.25))
    date_cols = 1 + max(3, int(remaining * 0.10))

    used = (
        long_str_cols
        + float_general_cols
        + float_scientific_cols
        + float_mixed_cols
        + int_cols
        + date_cols
    )
    str_cols = total_cols - used
    if str_cols < 1:
        overflow = 1 - str_cols
        int_cols = max(1, int_cols - overflow)
        str_cols = 1

    layout = ColumnLayout(
        str_cols=str_cols,
        long_str_cols=long_str_cols,
        float_general_cols=float_general_cols,
        float_scientific_cols=float_scientific_cols,
        float_mixed_cols=float_mixed_cols,
        int_cols=int_cols,
        date_cols=date_cols,
    )
    assert layout.total() == total_cols, "Column layout mismatch"
    return layout


def random_tokens(rng: np.random.Generator, rows: int, width: int) -> np.ndarray:
    idx = rng.integers(0, len(ALPHABET), size=(rows, width), dtype=np.int16)
    chars = ALPHABET[idx]
    return np.apply_along_axis(lambda x: "".join(x.tolist()), 1, chars)


def iter_columns(layout: ColumnLayout) -> Iterable[tuple[str, str]]:
    """Yield columns with one representative of each type first, then the rest."""
    groups: list[tuple[str, int, str]] = [
        ("str", layout.str_cols, "str"),
        ("long_str", layout.long_str_cols, "long_str"),
        ("float_general", layout.float_general_cols, "float_general"),
        ("float_scientific", layout.float_scientific_cols, "float_scientific"),
        ("float_mixed", layout.float_mixed_cols, "float_mixed"),
        ("int", layout.int_cols, "int"),
        ("date", layout.date_cols, "date"),
    ]

    for kind, count, prefix in groups:
        if count > 0:
            yield (kind, f"{prefix}_000")

    for kind, count, prefix in groups:
        for i in range(1, count):
            yield (kind, f"{prefix}_{i:03d}")


def generate_column(
    rng: np.random.Generator,
    kind: str,
    rows: int,
    start_row: int,
) -> np.ndarray:
    if kind == "str":
        return random_tokens(rng, rows, 12)
    if kind == "long_str":
        prefix = np.array([f"r{start_row + i:010d}_" for i in range(rows)], dtype=object)
        suffix = random_tokens(rng, rows, 189)
        return np.char.add(prefix.astype(str), suffix.astype(str))
    if kind == "float_general":
        return rng.normal(loc=0.0, scale=1000.0, size=rows)
    if kind == "float_scientific":
        return rng.normal(loc=0.0, scale=1.0, size=rows) * np.power(
            10.0, rng.integers(-9, 9, size=rows)
        )
    if kind == "float_mixed":
        vals = rng.normal(loc=10.0, scale=250.0, size=rows)
        scientific_mask = rng.random(rows) < 0.5
        out = np.empty(rows, dtype=object)
        out[scientific_mask] = [f"{v:.8e}" for v in vals[scientific_mask]]
        out[~scientific_mask] = [f"{v:.6f}" for v in vals[~scientific_mask]]
        return out
    if kind == "int":
        return rng.integers(-(2**31), 2**31 - 1, size=rows, dtype=np.int64)
    if kind == "date":
        day_offsets = rng.integers(0, 3650, size=rows, dtype=np.int32)
        base = np.datetime64("2015-01-01")
        timestamps = base + day_offsets.astype("timedelta64[D]")
        return pd.Series(timestamps).dt.strftime("%Y-%m-%d").to_numpy()
    raise ValueError(f"Unknown column kind: {kind}")


def generate_chunk(
    rng: np.random.Generator,
    layout: ColumnLayout,
    start_row: int,
    rows: int,
) -> pd.DataFrame:
    columns: dict[str, np.ndarray] = {}
    for kind, col in iter_columns(layout):
        columns[col] = generate_column(rng, kind, rows, start_row)
    return pd.DataFrame(columns)


def generate_dataset(
    spec: DatasetSpec,
    output_dir: Path,
    chunk_rows: int,
    seed: int,
) -> None:
    output_dir.mkdir(parents=True, exist_ok=True)
    output_path = output_dir / spec.filename
    layout = build_layout(spec.cols)
    rng = np.random.default_rng(seed)

    if output_path.exists():
        output_path.unlink()

    print(f"[generate] {spec.name} -> {output_path}")
    print(f"           layout={layout}")

    rows_written = 0
    first = True
    while rows_written < spec.rows:
        current = min(chunk_rows, spec.rows - rows_written)
        chunk = generate_chunk(
            rng=rng,
            layout=layout,
            start_row=rows_written,
            rows=current,
        )
        chunk.to_csv(output_path, mode="w" if first else "a", index=False, header=first)
        first = False
        rows_written += current
        print(f"           rows_written={rows_written}/{spec.rows}")


def main() -> None:
    args = parse_args()
    output_dir = Path(args.output_dir)

    for key in args.datasets:
        generate_dataset(
            spec=SPECS[key],
            output_dir=output_dir,
            chunk_rows=args.chunk_rows,
            seed=args.seed + DATASET_SEED_OFFSETS[key],
        )

    print("[done] all requested datasets generated")


if __name__ == "__main__":
    main()
