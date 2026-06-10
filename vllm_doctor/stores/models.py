from sqlalchemy import Text
from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column


class Base(DeclarativeBase):
    pass


class Run(Base):
    """A persisted diagnosis run. Shared schema for every SQL backend (sqlite, postgres)."""

    __tablename__ = "runs"

    id: Mapped[str] = mapped_column(primary_key=True)
    saved_at: Mapped[str]
    model_name: Mapped[str | None]
    client_mode: Mapped[str]
    health: Mapped[str]
    fired_count: Mapped[int]
    report: Mapped[str] = mapped_column(Text)
