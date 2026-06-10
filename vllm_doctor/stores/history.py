from datetime import datetime, timezone

import uuid_utils
from pydantic import BaseModel
from sqlalchemy import URL, create_engine, select
from sqlalchemy.orm import Session

from vllm_doctor.models import ClientMode, DiagnosisResult, Health
from vllm_doctor.stores.models import Base, Run


class RunSummary(BaseModel):
    """Summary of a stored run, for listing without loading the full report."""

    run_id: str
    saved_at: str
    model_name: str | None
    client_mode: ClientMode
    health: Health
    fired_count: int


class HistoryStore:
    """Diagnosis history backed by SQLAlchemy and configured by database URL."""

    def __init__(self, url: str | URL) -> None:
        self._engine = create_engine(url)
        Base.metadata.create_all(self._engine)

    def __enter__(self) -> "HistoryStore":
        return self

    def __exit__(self, *args: object) -> None:
        self.close()

    def save(self, result: DiagnosisResult) -> str:
        run = Run(
            id=str(uuid_utils.uuid7()),
            saved_at=datetime.now(timezone.utc).isoformat(),
            model_name=result.context.model_name,
            client_mode=result.context.client_mode.value,
            health=result.health.value,
            fired_count=sum(1 for check in result.checks if check.finding is not None),
            report=result.model_dump_json(),
        )
        with Session(self._engine) as session:
            session.add(run)
            session.commit()
            return run.id

    def list(self) -> list[RunSummary]:
        with Session(self._engine) as session:
            runs = session.scalars(select(Run).order_by(Run.saved_at.desc(), Run.id.desc())).all()
            return [
                RunSummary(
                    run_id=run.id,
                    saved_at=run.saved_at,
                    model_name=run.model_name,
                    client_mode=ClientMode(run.client_mode),
                    health=Health(run.health),
                    fired_count=run.fired_count,
                )
                for run in runs
            ]

    def get(self, run_id: str) -> DiagnosisResult | None:
        with Session(self._engine) as session:
            run = session.get(Run, run_id)
            return DiagnosisResult.model_validate_json(run.report) if run is not None else None

    def delete(self, run_id: str) -> bool:
        with Session(self._engine) as session:
            run = session.get(Run, run_id)
            if run is None:
                return False
            session.delete(run)
            session.commit()
            return True

    def close(self) -> None:
        self._engine.dispose()
