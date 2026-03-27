#!/usr/bin/env bash
# F3-T3: Run committed corpus through import + canonical stats and generate uncertainty report.
# Requires CHECK_MATE_DATABASE_URL with bootstrapped dev DB.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BACKEND_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
DEFAULT_DATABASE_URL="host=localhost port=5433 user=postgres password=postgres dbname=check_mate_dev"
CHECK_MATE_DATABASE_URL="${CHECK_MATE_DATABASE_URL:-$DEFAULT_DATABASE_URL}"
export CHECK_MATE_DATABASE_URL

REPORT_FILE="${BACKEND_DIR}/../docs/runtime_uncertainty_report.md"

resolve_psql() {
  local candidates=(
    "${PSQL_BIN:-}"
    "$(command -v psql 2>/dev/null || true)"
    "/opt/homebrew/opt/postgresql@16/bin/psql"
    "/opt/homebrew/bin/psql"
  )
  for c in "${candidates[@]}"; do
    if [[ -n "$c" && -x "$c" ]]; then
      printf '%s\n' "$c"
      return 0
    fi
  done
  echo "psql not found" >&2
  exit 1
}

PSQL="$(resolve_psql)"

echo "[report] Generating runtime uncertainty report from committed corpus..."

# Count core entities
HAND_COUNT=$("$PSQL" "$CHECK_MATE_DATABASE_URL" -Atqc "SELECT count(*) FROM core.hands" 2>/dev/null || echo "N/A")
TOURNAMENT_COUNT=$("$PSQL" "$CHECK_MATE_DATABASE_URL" -Atqc "SELECT count(*) FROM core.tournaments" 2>/dev/null || echo "N/A")
ELIMINATION_COUNT=$("$PSQL" "$CHECK_MATE_DATABASE_URL" -Atqc "SELECT count(*) FROM derived.hand_eliminations" 2>/dev/null || echo "N/A")

# Parse issues by severity + code
PARSE_ISSUES=$("$PSQL" "$CHECK_MATE_DATABASE_URL" -Atqc "
    SELECT severity, code, count(*)
    FROM core.parse_issues
    GROUP BY severity, code
    ORDER BY severity, count(*) DESC" 2>/dev/null || echo "N/A")

# Uncertain/inconsistent resolution states
UNCERTAIN_RESOLUTIONS=$("$PSQL" "$CHECK_MATE_DATABASE_URL" -Atqc "
    SELECT
      count(*) FILTER (WHERE chip_conservation_ok IS FALSE) AS chip_conservation_fail,
      count(*) FILTER (WHERE pot_conservation_ok IS FALSE) AS pot_conservation_fail,
      count(*) FILTER (WHERE invariant_errors::text != '[]') AS has_invariant_errors
    FROM derived.hand_state_resolutions" 2>/dev/null || echo "N/A")

# Elimination certainty states
ELIM_CERTAINTY=$("$PSQL" "$CHECK_MATE_DATABASE_URL" -Atqc "
    SELECT certainty_state, count(*)
    FROM derived.hand_eliminations
    GROUP BY certainty_state
    ORDER BY count(*) DESC" 2>/dev/null || echo "N/A")

# Boundary resolution states
BOUNDARY_STATES=$("$PSQL" "$CHECK_MATE_DATABASE_URL" -Atqc "
    SELECT boundary_resolution_state, count(*)
    FROM derived.mbr_stage_resolution
    WHERE entered_boundary_zone IS TRUE
    GROUP BY boundary_resolution_state
    ORDER BY count(*) DESC" 2>/dev/null || echo "N/A")

# FT helper coverage
FT_HELPER=$("$PSQL" "$CHECK_MATE_DATABASE_URL" -Atqc "
    SELECT
      count(*) AS total,
      count(*) FILTER (WHERE reached_ft_exact) AS reached_ft,
      count(*) FILTER (WHERE ft_started_incomplete IS TRUE) AS incomplete_ft,
      count(*) FILTER (WHERE boundary_resolution_state = 'exact') AS exact_boundary,
      count(*) FILTER (WHERE boundary_resolution_state = 'uncertain') AS uncertain_boundary
    FROM derived.mbr_tournament_ft_helper" 2>/dev/null || echo "N/A")

# Tournament hand order coverage
ORDER_COVERAGE=$("$PSQL" "$CHECK_MATE_DATABASE_URL" -Atqc "
    SELECT
      count(*) AS total_hands,
      count(*) FILTER (WHERE tournament_hand_order IS NOT NULL) AS with_order,
      count(*) FILTER (WHERE tournament_hand_order IS NULL) AS without_order
    FROM core.hands" 2>/dev/null || echo "N/A")

# Generate report
cat > "$REPORT_FILE" << 'HEADER'
# Runtime Uncertainty Report

Generated from committed GG MBR corpus (9 HH + 9 TS + edge matrix).

## Corpus Coverage

HEADER

cat >> "$REPORT_FILE" << EOF
| Entity | Count |
|--------|-------|
| Tournaments | ${TOURNAMENT_COUNT} |
| Hands | ${HAND_COUNT} |
| Eliminations | ${ELIMINATION_COUNT} |

## Parse Issues

| Severity | Code | Count |
|----------|------|-------|
EOF

if [[ "$PARSE_ISSUES" != "N/A" && -n "$PARSE_ISSUES" ]]; then
  echo "$PARSE_ISSUES" | while IFS='|' read -r sev code cnt; do
    echo "| ${sev} | ${code} | ${cnt} |" >> "$REPORT_FILE"
  done
else
  echo "| (none) | - | 0 |" >> "$REPORT_FILE"
fi

cat >> "$REPORT_FILE" << EOF

## Hand State Resolutions

\`\`\`
${UNCERTAIN_RESOLUTIONS}
\`\`\`

## Elimination Certainty States

| State | Count |
|-------|-------|
EOF

if [[ "$ELIM_CERTAINTY" != "N/A" && -n "$ELIM_CERTAINTY" ]]; then
  echo "$ELIM_CERTAINTY" | while IFS='|' read -r state cnt; do
    echo "| ${state} | ${cnt} |" >> "$REPORT_FILE"
  done
fi

cat >> "$REPORT_FILE" << EOF

## Boundary Resolution States

| State | Count |
|-------|-------|
EOF

if [[ "$BOUNDARY_STATES" != "N/A" && -n "$BOUNDARY_STATES" ]]; then
  echo "$BOUNDARY_STATES" | while IFS='|' read -r state cnt; do
    echo "| ${state} | ${cnt} |" >> "$REPORT_FILE"
  done
else
  echo "| (no boundary hands) | 0 |" >> "$REPORT_FILE"
fi

cat >> "$REPORT_FILE" << EOF

## FT Helper Coverage

\`\`\`
${FT_HELPER}
\`\`\`

## Tournament Hand Order Coverage

\`\`\`
${ORDER_COVERAGE}
\`\`\`

## Known Limitations

- Extended real corpus beyond committed pack has not been run yet.
- Timezone-normalized timestamps remain NULL; ordering relies on local timestamps.
- \`is_nut_hand\` and \`is_nut_draw\` are now exact under \`STREET_HAND_STRENGTH_NUT_POLICY = hand_and_draw\`; runtime filters for nut predicates still remain intentionally unsupported.
- Big KO / adjusted money stats are frequency-weighted estimates, not posterior reconstructions.
- Same-timestamp hands from different tables may produce arbitrary ordering within their tie class.

## Backlog Items

After extended corpus run, update this section with:
- Top unsupported syntactic variants
- New parse issue classes
- Boundary ambiguity rates on real data
- KO attempt false-positive analysis
EOF

echo "[report] Report written to: ${REPORT_FILE}"
