import uuid
from datetime import datetime

from sqlalchemy import DateTime, Text, Uuid
from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column


class Base(DeclarativeBase):
    pass


class Run(Base):
    """A persisted diagnosis run. Shared schema for every SQL backend (sqlite, postgres)."""

    __tablename__ = "runs"

    id: Mapped[uuid.UUID] = mapped_column(Uuid(), primary_key=True)
    saved_at: Mapped[datetime] = mapped_column(DateTime(timezone=True))
    model_name: Mapped[str | None]
    client_mode: Mapped[str]
    health: Mapped[str]
    fired_count: Mapped[int]
    report: Mapped[str] = mapped_column(Text)
