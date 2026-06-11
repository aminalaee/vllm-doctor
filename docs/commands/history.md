# `vllm-doctor history`

Review saved diagnosis runs.

## `vllm-doctor history list`

List all saved runs as a compact table.

### Usage

```shell
vllm-doctor history list [OPTIONS]
```

### Options

| Option            | Default | Description                      |
| ----------------- | ------- | -------------------------------- |
| `-o`, `--output`  | `text`  | Output format: `text` or `json`. |
| `-v`, `--verbose` | False   | Show additional columns (Mode).  |
| `-c`, `--config`  | —       | Path to config file.             |

### Text output (default)

```shell
vllm-doctor history list
```

Columns shown by default:

- **Run ID** — UUID7, time-ordered
- **Time** — saved at (YYYY-MM-DD HH:MM)
- **Model** — model name, or `—` if none
- **Health** — colored ok / info / warning / critical
- **Fired** — number of findings that fired

### Verbose

Add `--verbose` to also show the **Mode** column (`prometheus` or `scrape`).

```shell
vllm-doctor history list --verbose
```

### JSON output

```shell
vllm-doctor history list --output json
```

Renders a JSON array of `RunSummary` objects.

## `vllm-doctor history show`

Re-render a stored diagnosis run using the same reporters as a live diagnosis.

### Usage

```shell
vllm-doctor history show [OPTIONS] RUN_ID
```

### Options

| Option            | Default | Description                                      |
| ----------------- | ------- | ------------------------------------------------ |
| `-o`, `--output`  | `text`  | Output format: `text` or `json`.                 |
| `-v`, `--verbose` | False   | Show observed metrics and per-replica breakdown. |
| `-c`, `--config`  | —       | Path to config file.                             |

### Text output (default)

```shell
vllm-doctor history show <run-id>
```

### JSON output

```shell
vllm-doctor history show <run-id> --output json
```

### Not found

If the run ID does not exist, exits with code 1 and prints an error to stderr.
