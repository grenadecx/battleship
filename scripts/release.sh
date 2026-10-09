#!/usr/bin/env bash
# Publishes a release: bumps the version in Cargo.toml, commits, tags and pushes.
# The pushed tag triggers .github/workflows/release.yml, which builds the binaries
# and creates the GitHub release.
#
# Usage: scripts/release.sh <patch|minor|major|VERSION> [--dry-run] [--yes]
#
#   patch, minor, major  bump that part of the current version
#   VERSION              an explicit version such as 1.2.0 or 1.0.0-rc.1
#                        (a suffix after "-" makes it a pre-release)
#   --dry-run            run every check, then stop before changing anything
#   --yes                do not ask for confirmation before pushing
set -euo pipefail

usage() {
    sed -n '/^# Usage:/,/^set /{/^set /d;s/^# \{0,1\}//;p;}' "$0"
    exit "${1:-0}"
}

die() {
    echo "error: $*" >&2
    exit 1
}

step() {
    echo
    echo "==> $*"
}

semver_re='^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$'

# Succeeds when version $1 is newer than version $2 (semver precedence).
is_newer() {
    local new_core=${1%%-*} old_core=${2%%-*}
    local new_pre='' old_pre=''
    [[ $1 == *-* ]] && new_pre=${1#*-}
    [[ $2 == *-* ]] && old_pre=${2#*-}
    if [[ $new_core != "$old_core" ]]; then
        [[ $(printf '%s\n%s\n' "$new_core" "$old_core" | sort -V | tail -n1) == "$new_core" ]]
        return
    fi
    # Same core: a final release is newer than its pre-releases.
    [[ -z $new_pre && -n $old_pre ]] && return 0
    [[ -z $new_pre || -z $old_pre || $new_pre == "$old_pre" ]] && return 1
    [[ $(printf '%s\n%s\n' "$new_pre" "$old_pre" | sort -V | tail -n1) == "$new_pre" ]]
}

bump=''
dry_run=false
assume_yes=false
for arg in "$@"; do
    case $arg in
        -h | --help) usage ;;
        --dry-run) dry_run=true ;;
        --yes | -y) assume_yes=true ;;
        -*) echo "error: unknown option $arg" >&2; usage 1 >&2 ;;
        *)
            [[ -z $bump ]] || die "only one version may be given"
            bump=$arg
            ;;
    esac
done
[[ -n $bump ]] || usage 1 >&2

cd "$(git rev-parse --show-toplevel)"

# Read the version the same way the release workflow does.
current=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
[[ $current =~ $semver_re ]] || die "Cargo.toml version '$current' is not a semantic version"

case $bump in
    patch | minor | major)
        IFS=. read -r major minor patch <<<"${current%%-*}"
        case $bump in
            patch) version="$major.$minor.$((patch + 1))" ;;
            minor) version="$major.$((minor + 1)).0" ;;
            major) version="$((major + 1)).0.0" ;;
        esac
        ;;
    *) version=${bump#v} ;;
esac
[[ $version =~ $semver_re ]] || die "'$version' is not a version like 1.2.3 or 1.2.3-rc.1"
is_newer "$version" "$current" || die "$version is not newer than the current version $current"
tag="v$version"

step "Checking the repository"
branch=$(git symbolic-ref --short HEAD 2>/dev/null || true)
[[ $branch == main ]] || die "releases are made from main, but you are on '${branch:-a detached HEAD}'"
[[ -z $(git status --porcelain) ]] || die "the working tree has uncommitted changes"
git var GIT_COMMITTER_IDENT >/dev/null 2>&1 || die "git has no user.name/user.email to commit with"
git fetch --quiet --tags origin main
[[ $(git rev-parse HEAD) == $(git rev-parse origin/main) ]] ||
    die "main is not in sync with origin/main; pull or push first"
! git rev-parse --quiet --verify "refs/tags/$tag" >/dev/null || die "tag $tag already exists"
echo "main is clean and up to date; $tag is free"

step "Running the CI checks"
cargo fmt --check
cargo clippy --all-targets --quiet -- -D warnings
cargo test --quiet

previous=$(git describe --tags --abbrev=0 2>/dev/null || true)
step "Release $current -> $version"
if [[ -n $previous ]]; then
    echo "Changes since $previous:"
    git log --oneline --no-decorate "$previous..HEAD" | sed 's/^/  /'
else
    echo "This is the first release."
fi
[[ $version == *-* ]] && echo "The '-' suffix makes this a pre-release."

if $dry_run; then
    echo
    echo "Dry run: nothing was changed."
    exit 0
fi
if ! $assume_yes; then
    echo
    read -r -p "Commit, tag and push $tag? [y/N] " answer
    [[ $answer == [yY] || $answer == [yY][eE][sS] ]] || die "aborted; nothing was changed"
fi

step "Bumping the version"
# Until the commit exists, a failure puts the version files back as they were.
trap 'git checkout --quiet HEAD -- Cargo.toml Cargo.lock; rm -f Cargo.toml.tmp; echo "error: release failed; Cargo.toml and Cargo.lock were restored" >&2' EXIT
awk -v version="$version" '
    !done && /^version *=/ { print "version = \"" version "\""; done = 1; next }
    { print }
' Cargo.toml >Cargo.toml.tmp && mv Cargo.toml.tmp Cargo.toml
cargo update --workspace --quiet
git add Cargo.toml Cargo.lock
git commit --quiet -m "chore: release $tag"
trap - EXIT
git tag -a "$tag" -m "Battleship $tag"
echo "Committed and tagged $tag"

step "Pushing"
# Atomic, so the tag never reaches GitHub without the commit it points to.
if ! git push --atomic origin main "$tag"; then
    echo >&2
    echo "The push failed. To undo the local release commit and tag:" >&2
    echo "  git tag -d $tag && git reset --hard HEAD~1" >&2
    exit 1
fi

echo
echo "Released $tag. The release workflow builds and publishes it from here."
origin=$(git remote get-url origin)
if [[ $origin =~ github\.com[:/]([^/]+/[^/]+)$ ]]; then
    repo=${BASH_REMATCH[1]%.git}
    echo "  https://github.com/$repo/actions"
    echo "  https://github.com/$repo/releases/tag/$tag"
fi
