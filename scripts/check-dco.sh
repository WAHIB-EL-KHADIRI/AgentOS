#!/usr/bin/env bash
# Verify that every non-merge commit in a range carries a Signed-off-by
# trailer matching its own author, as required by .github/CLA.md.
#
# Usage:
#   scripts/check-dco.sh                    # origin/main..HEAD
#   scripts/check-dco.sh <base> <head>
#
# Only commits inside the given range are examined. History that predates
# the policy is never walked, so existing commits cannot be failed by it.

set -euo pipefail

# Accept "signed-off-by" in any casing; git writes it capitalised, some
# editors and tools do not.
shopt -s nocasematch

base="${1:-origin/main}"
head="${2:-HEAD}"

if ! merge_base=$(git merge-base "$base" "$head" 2>/dev/null); then
  echo "error: no merge base between '$base' and '$head'" >&2
  exit 2
fi

commits=()
while IFS= read -r line; do
  [ -n "$line" ] && commits+=("$line")
done < <(git rev-list --no-merges "$merge_base..$head")

if [ "${#commits[@]}" -eq 0 ]; then
  echo "No commits to check in ${base}..${head}."
  exit 0
fi

echo "Checking ${#commits[@]} commit(s) in ${base}..${head}"
echo

missing=()
for sha in "${commits[@]}"; do
  short=$(git rev-parse --short "$sha")
  name=$(git show -s --format='%an' "$sha")
  email=$(git show -s --format='%ae' "$sha")

  # A bot cannot agree to a licence agreement. Its commits come from
  # automation the maintainer already controls, so they are out of scope.
  case "$name" in
    *'[bot]')
      echo "  skip  ${short}  bot: ${name}"
      continue
      ;;
  esac

  message=$(git show -s --format='%B' "$sha")

  # Matched with bash's own pattern operator rather than grep. Piping into
  # `grep -q` breaks under `set -o pipefail`, because grep closes the pipe on
  # its first match and the resulting SIGPIPE marks the whole pipeline failed
  # even when the trailer is present. Keeping it in-process also spawns no
  # subprocess per commit, and the quoted expansion means a name containing
  # glob or regex metacharacters is compared literally.
  if [[ "$message" == *"Signed-off-by: ${name} <${email}>"* ]]; then
    echo "  ok    ${short}  ${name} <${email}>"
  else
    echo "  FAIL  ${short}  ${name} <${email}>"
    missing+=("$short")
  fi
done

if [ "${#missing[@]}" -gt 0 ]; then
  echo
  echo "-------------------------------------------------------------------"
  echo "${#missing[@]} commit(s) lack a Signed-off-by trailer matching the author:"
  printf '  %s\n' "${missing[@]}"
  cat <<'EOF'

Every commit must carry a trailer of exactly this form, naming the commit's
own author:

  Signed-off-by: Your Name <your.email@example.com>

Sign new commits automatically with:

  git commit -s -m "your message"

To fix commits that are already written:

  git commit --amend -s --no-edit          # the most recent commit only
  git rebase --signoff origin/main         # every commit on this branch

then push the branch again.

What the trailer means: it is the Developer Certificate of Origin. You are
certifying that you wrote the work, or otherwise have the right to submit it
under the project's licence. It is not, on its own, agreement to the
Contributor Licence Agreement -- see .github/CLA.md, which is accepted
separately and only once.

Set your identity so the trailer always matches:

  git config user.name  "Your Name"
  git config user.email "your.email@example.com"
EOF
  exit 1
fi

echo
echo "All commits are signed off."
