# Test Data Generation

Python/Pandas generator for synthetic CSV datasets (`scripts/generate_test_data.py`).  
See **[CSV parsing & display](reference/csv-parsing.md)** for how column types and alignment are inferred.

## Generated Datasets

Configured in `SPECS` inside the script:

| Key | Output file |
|-----|-------------|
| `1000x100` | `test-data/generated/test_1000x100.csv` |
| `10000x1000` | `test-data/generated/test_10000x1000.csv` |

Default output directory: `test-data/generated/`.

## Column Types Included

Each dataset contains a mix of:

- standard string columns
- a few long string columns (~200 characters)
- float columns (general decimal format)
- float columns with scientific-scale values
- mixed-format float-like columns (some decimal, some scientific as text)
- integer columns
- date columns (`YYYY-MM-DD`)

The **first seven columns** are always one of each type (`str_000`, `long_str_000`, `float_general_000`, …), then the rest follow the layout ratios in `build_layout()`.

## Run via Pixi

Install/update dependencies:

```bash
pixi install
```

Generate all default datasets:

```bash
pixi run gen-test-data
```

Generate only selected sizes:

```bash
pixi run gen-test-data --datasets 1000x100 10000x1000
```

Change output directory and chunk size:

```bash
pixi run gen-test-data --output-dir test-data/custom --chunk-rows 5000
```

## Implementation notes

- Rows are built as column dicts and assembled with `pd.DataFrame(columns)` per chunk (avoids pandas fragmentation warnings).
- Large files are written in chunks (`--chunk-rows`, default 20_000).

## Script Location

- `scripts/generate_test_data.py`
