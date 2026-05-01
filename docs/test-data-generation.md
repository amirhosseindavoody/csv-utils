# Test Data Generation

This project includes a Python/Pandas generator for synthetic CSV datasets used to test parser behavior and performance.

## Generated Datasets

The script generates these CSV shapes:

- `1000x100`
- `10000x1000`
- `1e6x1000`

Output files are written to `test-data/generated/` by default:

- `test-data/generated/test_1000x100.csv`
- `test-data/generated/test_10000x1000.csv`
- `test-data/generated/test_1000000x1000.csv` (see note below)

## Column Types Included

Each dataset contains a mix of:

- standard string columns
- a few long string columns (~200 characters)
- float columns (general decimal format)
- float columns with scientific-scale values
- mixed-format float-like columns (some decimal, some scientific as text)
- integer columns
- date columns (`YYYY-MM-DD`)

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

## Notes on Large Files

- The `1e6x1000` file can be extremely large and may require significant disk space and generation time.
- The generator writes in chunks to avoid loading all rows into memory at once.
- Use `--datasets` to generate smaller files first if you are iterating quickly.

## Script Location

- `scripts/generate_test_data.py`
