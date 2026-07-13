# Test data generation

Python/Pandas generator for synthetic CSV datasets (`scripts/generate_test_data.py`).
See [CSV parsing & display](reference/csv-parsing.md) for how column types and alignment are inferred.

## Datasets

Configured in `SPECS` inside the script:

| Key | Output file |
|-----|-------------|
| `1000x100` | `test-data/generated/test_1000x100.csv` |
| `10000x1000` | `test-data/generated/test_10000x1000.csv` |

Default output directory: `test-data/generated/`.

## Column mix

Each dataset includes:

- Standard string columns
- A few long string columns (~200 characters)
- Float columns (general decimal format)
- Float columns with scientific-scale values
- Mixed-format float-like columns (decimal and scientific as text)
- Integer columns
- Date columns (`YYYY-MM-DD`)

The first seven columns are one of each type (`str_000`, `long_str_000`, `float_general_000`, …); the rest follow the layout ratios in `build_layout()`.

## Run

```bash
pixi install
pixi run gen-test-data
pixi run gen-test-data --datasets 1000x100 10000x1000
pixi run gen-test-data --output-dir test-data/custom --chunk-rows 5000
```

## Notes

- Rows are built as column dicts and assembled with `pd.DataFrame(columns)` per chunk.
- Large files are written in chunks (`--chunk-rows`, default 20_000).
- Generated files are gitignored under `test-data/generated/`.
