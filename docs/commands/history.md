# History and persistence

vLLM Doctor can persist diagnosis runs to a local database, watch a target for state transitions and log a change-log, and re-render stored runs. Everything here is opt-in and local-only — no hosted service, no auth.

## Quick start

```shell
# 1. Initialize the database (once, after install or upgrade)
vllm-doctor migrate

# 2. Save a one-shot run
vllm-doctor diagnose http://localhost:8000/metrics --save

# 3. Review it later
vllm-doctor history list
vllm-doctor history show <run-id>
```

The database is a single SQLite file at `~/.vllm-doctor/vllm_doctor.db` by default. See [Configuration](../configuration.md#database) to point at a different file.

Before the first `--save` (and after every upgrade that ships a schema change), run [`vllm-doctor migrate`](migrate.md) once to create or update the schema. It is idempotent.

## `vllm-doctor diagnose --save`

Persist a one-shot diagnosis to the local database:

```shell
vllm-doctor diagnose http://localhost:8000/metrics --save
```

The run id is printed to stderr so it does not corrupt text or JSON output:

```
Saved run: 019ebb28-8d58-7023-ab27-7e392cd0dc44
```

Use the run id with `history show` to re-render the same diagnosis later.

## `vllm-doctor diagnose --save --watch`

Combine `--save` and `--watch` to produce a change-log: a new run is saved only when the diagnosis state transitions (health changes, or the set of firing rules changes). The first tick is always saved as the initial state.

```shell
vllm-doctor diagnose http://localhost:8000/metrics --save --watch
```

Save messages are printed to stderr:

```
Saved run: 019ebb28-8d58-... (initial)
Saved run: 019ebb28-8e22-... ok → warning
Saved run: 019ebb28-8f01-... (rules changed)
```

The full set of `--diagnose` options (URL, model, output, etc.) work the same way with `--save` and `--watch`. See [`vllm-doctor diagnose`](diagnose.md) for the rest.

## `vllm-doctor history list`

List all saved runs as a compact table.

### Usage

```shell
vllm-doctor history list [OPTIONS]
```

### Options

| Option            | Default | Description                                       |
| ----------------- | ------- | ------------------------------------------------- |
| `-o`, `--output`  | `text`  | Output format: `text` or `json`.                  |
| `-v`, `--verbose` | False   | Show additional columns (Source, Engine, Target). |
| `-c`, `--config`  | —       | Path to config file.                              |

### Text output (default)

```shell
vllm-doctor history list
```

Columns shown by default:

- **Run ID** — unique identifier for the saved run
- **Time** — saved at (YYYY-MM-DD HH:MM)
- **Model** — model name, or `—` if none
- **Health** — colored ok / info / warning / critical
- **Fired** — number of findings that fired

### Verbose

Add `--verbose` to also show the **Source** (`prometheus` or `direct_scrape`), **Engine** (currently always `vllm`), and **Target** (operator-provided target ID, or `—` if none) columns.

```shell
vllm-doctor history list --verbose
```

### JSON output

```shell
vllm-doctor history list --output json
```

Renders a JSON array containing one summary per saved run.

## `vllm-doctor history show`

Display a stored diagnosis using the same text or JSON format as a live run.

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

If the run ID does not exist, exits with code 2 and prints an error to stderr.
