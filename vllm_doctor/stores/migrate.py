from pathlib import Path

from alembic import command
from alembic.config import Config as AlembicConfig
from sqlalchemy import URL, create_engine


def run_migrations(url: str | URL) -> None:
    """Run Alembic migrations against the given database URL."""
    engine = create_engine(url)
    migrations_dir = Path(__file__).parent / "migrations"
    cfg = AlembicConfig()
    cfg.set_main_option("script_location", str(migrations_dir))
    with engine.begin() as connection:
        cfg.attributes["connection"] = connection
        command.upgrade(cfg, "head")
    engine.dispose()
