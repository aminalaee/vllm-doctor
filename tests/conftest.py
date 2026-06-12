import os
from collections.abc import Generator

import pytest
from sqlalchemy import create_engine, text

from vllm_doctor.stores.models import Base


@pytest.fixture
def db_url() -> Generator[str, None, None]:
    url = os.environ.get("TEST_DATABASE_URI", "sqlite:///vllm_doctor.db")
    yield url


@pytest.fixture(autouse=True)
def _provision_db(db_url: str) -> Generator[None, None, None]:
    engine = create_engine(db_url)
    try:
        Base.metadata.drop_all(engine)
        with engine.begin() as conn:
            conn.execute(text("DROP TABLE IF EXISTS alembic_version"))
        yield
        Base.metadata.drop_all(engine)
        with engine.begin() as conn:
            conn.execute(text("DROP TABLE IF EXISTS alembic_version"))
    finally:
        engine.dispose()
