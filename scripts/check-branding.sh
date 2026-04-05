#!/usr/bin/env bash
# check-branding.sh — fail CI if stale org-name references remain in markdown files.
#
# Scans every *.md file in the repo and exits non-zero if any badge/URL still
# points to the old `ultraworkers` organisation instead of `midnghtsapphire`.
# Planning/description text that merely *mentions* the old name is allowed and
# excluded via a path allowlist below.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Files that may legitimately reference the old name for documentation purposes.
# These are planning/history documents, not user-facing badge/URL references.
ALLOWLIST=(
  "docs/claw-code/SCRUM_PROJECT_PLAN.md"
  "docs/claw-code/CHANGE_PROPOSAL.md"
)

# Find all markdown files that mention the old org name
ALL_BAD=$(grep -r --include="*.md" -l "ultraworkers" "${REPO_ROOT}" 2>/dev/null || true)

# Filter out allowed files
BAD=""
while IFS= read -r file; do
  [[ -z "${file}" ]] && continue
  rel="${file#"${REPO_ROOT}/"}"
  allowed=false
  for allowed_path in "${ALLOWLIST[@]}"; do
    if [[ "${rel}" == "${allowed_path}" ]]; then
      allowed=true
      break
    fi
  done
  if [[ "${allowed}" == "false" ]]; then
    BAD="${BAD}${file}"$'\n'
  fi
done <<< "${ALL_BAD}"

BAD="${BAD%$'\n'}"  # trim trailing newline

if [[ -n "${BAD}" ]]; then
  echo "❌ Stale branding found in the following markdown file(s):"
  echo "${BAD}"
  echo ""
  echo "Replace 'ultraworkers' with 'midnghtsapphire' in any badge/URL references."
  exit 1
fi

echo "✅ No stale branding references found."
