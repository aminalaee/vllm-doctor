# `vllm-doctor migrate`

Initialize or update the configured history database. Safe to run repeatedly.

## Usage

```shell
vllm-doctor migrate [OPTIONS]
```

## Options

| Option           | Default | Description          |
| ---------------- | ------- | -------------------- |
| `-c`, `--config` | —       | Path to config file. |

## When to run

- **After first install** — creates the local history database.
- **After upgrading vLLM Doctor** — updates the database when required by the
  new version.
- **After changing `[database].url`** — if you point at a fresh database, run `migrate` to initialize it.

If the database is already current, the command makes no changes.

## Examples

Initialize the default local database:

```shell
vllm-doctor migrate
```

Initialize a custom database (the `url` value comes from your config; see [Configuration](../configuration.md#database)):

```shell
vllm-doctor migrate --config ./vllm-doctor.toml
```

## See also

- [History and persistence](history.md) — what runs are saved, how to review them.
- [Configuration](../configuration.md#database) — the `[database] url` setting.
