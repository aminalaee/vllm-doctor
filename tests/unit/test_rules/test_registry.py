from vllm_doctor.rules.utils.registry import _RULES


def test_all_rules_have_unique_ids() -> None:
    """Ensure all rules have non-empty, unique IDs for stable automation contracts."""
    rule_ids = []

    for rule_class in _RULES:
        assert hasattr(rule_class, "id"), f"{rule_class.__name__} missing id attribute"
        assert isinstance(rule_class.id, str), f"{rule_class.__name__}.id must be string"
        assert len(rule_class.id) > 0, f"{rule_class.__name__}.id is empty"
        rule_ids.append(rule_class.id)

    assert len(rule_ids) == len(set(rule_ids)), "Duplicate rule IDs found"
