def rising(current: float | None, previous: float | None, threshold: float = 0.10) -> bool:
    """Returns True when current exceeds previous by more than threshold (relative)."""
    if current is None or previous is None or previous == 0:
        return False
    return (current - previous) / abs(previous) > threshold


def falling(current: float | None, previous: float | None, threshold: float = 0.10) -> bool:
    """Returns True when current is below previous by more than threshold (relative)."""
    if current is None or previous is None or previous == 0:
        return False
    return (current - previous) / abs(previous) < -threshold
