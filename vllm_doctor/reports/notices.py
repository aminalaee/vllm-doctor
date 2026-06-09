from vllm_doctor.metrics import MODEL_LABEL, label_values
from vllm_doctor.models import ClientMode, DiagnosisResult

SCRAPE_MODE_NOTICE = "TTFT, TPOT and Queue Latency rules require Prometheus — connect to Prometheus for full analysis."
MULTI_MODEL_NOTICE = (
    "Multiple models found in this target — metrics are aggregated across all of them. "
    "Pass --model to scope the report to one."
)


def resolve_notices(result: DiagnosisResult) -> list[str]:
    """All mode/data notices that apply to a report.

    Notices are advisory caveats about how to read the report — e.g. that scrape mode
    omits latency rules, or that metrics blend across models when no `--model` is given.
    """
    notices: list[str] = []
    if result.context.client_mode == ClientMode.scrape:
        notices.append(SCRAPE_MODE_NOTICE)
    if result.context.model_name is None and len(label_values(result.metric_series, MODEL_LABEL)) > 1:
        notices.append(MULTI_MODEL_NOTICE)
    return notices
