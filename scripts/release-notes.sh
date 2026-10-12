#!/bin/sh
# Prints Markdown release notes for a git range, grouped by conventional
# commit type. The output depends only on the commits in the range, so the
# same range always produces the same notes.
#
#   scripts/release-notes.sh v0.1.0 v0.2.0
#   scripts/release-notes.sh v0.1.0          # up to HEAD
#   scripts/release-notes.sh                 # whole history
#
# Commits whose body contains "BREAKING CHANGE:" or whose type ends in
# "!" (feat!: …) are also listed under "Breaking changes", each with its
# "BREAKING CHANGE:" paragraphs. Commits whose body says "deprecated" are
# also listed under "Deprecated", each with the sentences that say it.
set -eu

from=${1:-}
to=${2:-HEAD}
range=$to
[ -n "$from" ] && range="$from..$to"

log=$(git log --reverse --format='%H%x1f%s%x1f%b%x1e' "$range")

section() { # title pattern
    lines=$(printf '%s' "$log" | awk -v RS='\036' -v FS='\037' -v pattern="$2" '
        { sub(/^\n/, "") }
        $2 ~ pattern {
            subject = $2
            sub(/^[a-z]+(\([^)]*\))?!?: /, "", subject)
            scope = ""
            if (match($2, /^[a-z]+\([^)]*\)/)) {
                scope = substr($2, RSTART, RLENGTH)
                sub(/^[a-z]+\(/, "", scope)
                sub(/\)$/, "", scope)
                scope = "**" scope ":** "
            }
            printf "- %s%s (%s)\n", scope, subject, substr($1, 1, 7)
        }')
    if [ -n "$lines" ]; then
        printf '### %s\n\n%s\n\n' "$1" "$lines"
    fi
}

breaking=$(printf '%s' "$log" | awk -v RS='\036' -v FS='\037' '
    { sub(/^\n/, "") }
    $2 ~ /^[a-z]+(\([^)]*\))?!: / || $3 ~ /BREAKING CHANGE:/ {
        printf "- %s (%s)\n", $2, substr($1, 1, 7)
        # Each "BREAKING CHANGE:" paragraph becomes one nested item.
        n = split($3, body, "\n")
        note = ""
        for (i = 1; i <= n; i++) {
            if (body[i] ~ /^BREAKING CHANGE:/) {
                if (note != "") printf "  - %s\n", note
                note = body[i]
                sub(/^BREAKING CHANGE:[ \t]*/, "", note)
            } else if (note != "" && body[i] ~ /^[ \t]*$/) {
                printf "  - %s\n", note
                note = ""
            } else if (note != "") {
                note = note " " body[i]
            }
        }
        if (note != "") printf "  - %s\n", note
    }')

# Commits whose body says "deprecated", with each sentence that does.
deprecated=$(printf '%s' "$log" | awk -v RS='\036' -v FS='\037' '
    { sub(/^\n/, "") }
    tolower($3) ~ /deprecated/ {
        printf "- %s (%s)\n", $2, substr($1, 1, 7)
        text = $3
        gsub(/\n/, " ", text)
        gsub(/BREAKING CHANGE:[ \t]*/, "", text)
        n = split(text, sentences, /[.;] /)
        for (i = 1; i <= n; i++) {
            if (tolower(sentences[i]) !~ /deprecated/) continue
            sentence = sentences[i]
            sub(/^[ \t]+/, "", sentence)
            sub(/[. \t]+$/, "", sentence)
            printf "  - %s.\n", sentence
        }
    }')

printf '## Changes %s\n\n' "${from:+since $from}"
[ -n "$breaking" ] && printf '### Breaking changes\n\n%s\n\n' "$breaking"
[ -n "$deprecated" ] && printf '### Deprecated\n\n%s\n\n' "$deprecated"
section "Features" '^feat'
section "Fixes" '^fix'
section "Performance" '^perf'
section "Documentation" '^docs'
section "Tests" '^test'
section "Build, packaging, and CI" '^(build|ci)'
section "Other" '^(refactor|chore|style|revert)'
