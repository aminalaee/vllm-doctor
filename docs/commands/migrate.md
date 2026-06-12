# `vllm-doctor migrate`

Run database migrations against the configured database. Idempotent — safe to run repeatedly.

## Usage

```shell
vllm-doctor migrate [OPTIONS]
```

## Options

| Option           | Default | Description          |
| ---------------- | ------- | -------------------- |
| `-c`, `--config` | —       | Path to config file. |

## When to run

- **After first install** — creates the local database and schema.
- **After upgrading vllm-doctor** — applies any new migrations to bring the schema up to date.
- **After changing `[database].url`** — if you point at a fresh database, run `migrate` to initialize it.

The command is a no-op when the schema is already at the latest version, so it is safe to run on every deploy.

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
