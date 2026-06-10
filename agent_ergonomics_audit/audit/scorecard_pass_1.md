# shape Agent Ergonomics Scorecard - Pass 1

## Summary

- Mode: full
- Surfaces inventoried: 74
- Recommendations applied: 4 / 4
- Intent corpus: 125 entries, 0 silent failures, 0 useless errors
- Version prepared: 0.7.0

## Scores

| Dimension | Before | After | Evidence |
|---|---:|---:|---|
| Self-documentation | 640 | 860 | `shape capabilities --json`, `shape robot-docs guide`, `shape --describe` |
| Output parseability | 780 | 900 | single-object JSON for triage/capabilities/doctor health |
| Error pedagogy | 520 | 820 | `shape doctor --fix` names exact alternatives |
| Intent inference | 610 | 820 | top-level first-try commands and `shape --json doctor health` |
| Regression resistance | 680 | 840 | Rust tests plus audit regression scripts |

## Residual Risk

The core compare path was intentionally left unchanged. Future passes should consider typo correction for high-frequency flag misspellings if that becomes a recurring support issue.

