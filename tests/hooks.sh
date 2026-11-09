#!/usr/bin/env bash
# The published hooks, installed by pre-commit into other projects, the way a
# user installs them. Needs git, rsync and pre-commit (set PRE_COMMIT to use
# another executable).
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
export PRE_COMMIT_HOME="$work/cache"
pre_commit=${PRE_COMMIT:-pre-commit}

fail() { echo "FAIL: $*" >&2; exit 1; }
# expect CODE COMMAND...: run it, keep its output in $out, check the exit code.
expect() {
  local want=$1; shift
  set +e; out=$("$@" 2>&1); local got=$?; set -e
  [ "$got" = "$want" ] || fail "expected exit $want, got $got from: $*"$'\n'"$out"
}
same() { [ "$(cat "$1")" = "$2" ] || fail "$1 is $(cat "$1"), expected $2"; }
init() {
  mkdir -p "$1"
  git -C "$1" init -q -b main
  git -C "$1" config user.email test@example.invalid
  git -C "$1" config user.name "Hook Test"
}

# This repository, as committed, is the hook source.
hook=$work/hook
rsync -a --exclude .git --exclude target "$root/" "$hook/"
init "$hook"
git -C "$hook" add .
git -C "$hook" commit -qm snapshot
rev=$(git -C "$hook" rev-parse HEAD)

# consumer NAME HOOK_ID: a project using one hook, as the current directory.
consumer() {
  init "$work/$1"
  cd "$work/$1"
  printf "repos:\n  - repo: '%s'\n    rev: %s\n    hooks:\n      - id: %s\n" "$hook" "$rev" "$2" > .pre-commit-config.yaml
}

echo "names, check only"
consumer check quranic-terminology
echo '{"exclude": ["excluded/**"]}' > .terminology.json
echo 'ayah = 1' > 'a space.py'
git add .
expect 0 "$pre_commit" install
expect 0 git commit -m Clean
echo 'sura = 1' > 'a space.py'
git add .
expect 1 git commit -m Rejected
[[ $out == *"a space.py:1: error sura → surah"* ]] || fail "no finding in: $out"
same 'a space.py' 'sura = 1'
# A clean working copy doesn't hide an invalid staged version...
echo 'surah = 1' > 'a space.py'
expect 1 git commit -m "Still rejected"
same 'a space.py' 'surah = 1'
git add .
# ...and invalid unstaged work doesn't block clean staged content.
echo 'sura = 1' > 'a space.py'
expect 0 git commit -m "Clean staged content"
same 'a space.py' 'sura = 1'
echo 'surah = 1' > 'a space.py'
expect 0 "$pre_commit" run --all-files
mkdir excluded
echo 'sura = 1' > excluded/bad.py
git add .
expect 0 git commit -m "Only excluded change"
git rm -q 'a space.py'
expect 0 git commit -m Deletion

echo "names, unsafe fix"
consumer fix quranic-terminology-unsafe-fix
printf '# the aya list\naya_number = 1\n' > model.py
git add .
expect 0 "$pre_commit" install
expect 1 git commit -m "Fixed but stopped"
[[ $out == *"Fixed 2 names"* ]] || fail "no fix reported: $out"
same model.py $'# the ayah list\nayah_number = 1'
git add .
expect 0 git commit -m Reviewed
echo 'from quran import Chapters' > model.py
git add .
expect 1 git commit -m "Unsafe import rename"
same model.py 'from quran import Surahs'

echo "data"
consumer data quranic-data
echo 'ayah = 1' > model.py
echo '{"surah": 2, "riwayah": "hafs_an_asim", "ayahs": [{"ayah": 1, "text": "x"}]}' > ok.json
git add .
expect 0 "$pre_commit" install
expect 0 git commit -m Clean
# A warning doesn't stop the commit, but it is shown.
python3 -c 'import json; print(json.dumps({"surah": 2, "ayahs": [{"ayah": a} for a in range(1, 286)]}))' > al-baqarah.json
git add .
expect 0 git commit -m "Warned"
[[ $out == *"ayah numbers that are not Kufi"* ]] || fail "the warning wasn't shown: $out"
echo '{"riwaya": "hafsh"}' > export.json
git add .
expect 1 git commit -m Rejected
[[ $out == *'`hafsh` is not a riwayah'* ]] || fail "no finding in: $out"

echo "all hooks behave as published"
