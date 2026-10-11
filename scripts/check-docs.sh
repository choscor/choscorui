#!/usr/bin/env bash
# Checks the Markdown documentation outside docs/specs/:
#
#   1. Each relative link target (inline or reference definition) is a file
#      or directory in the repository, with the exact case. A #anchor on a
#      link to a Markdown file matches a heading in that file (GitHub slug
#      rules). HTML href and src attributes are not checked.
#   2. No banned history terms or checkbox list items outside fenced code.
#   3. In the ASD-STE100 scope (all checked files except CLAUDE.md and
#      .claude/), no prose sentence has more than 25 words. A code span
#      counts as one word. A `<!-- check-docs: no-sentence-check (reason) -->`
#      comment turns this check off until the next heading. Use it only for
#      text that must stay verbatim, such as standard license wording.
#
# Prints `file:line: message` for each finding and exits 1 when there is a
# finding (2 when git fails). Runs with macOS bash 3.2 and BSD awk; it needs
# no network.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
export LC_ALL=C

read -r -d '' program <<'AWK' || true
# Reads the repository paths (files and their parent directories) once.
# The comparison is case-exact, because GitHub paths are case-sensitive
# even when the local file system is not.
function load_paths(    line) {
    while ((getline line < paths) > 0) known[line] = 1
    close(paths)
    known["."] = 1
}

# Removes "." and ".." segments. Returns "." for the root and "" for a
# path above the root.
function normalize(path,    n, parts, out, depth, i, k) {
    n = split(path, parts, "/")
    depth = 0
    for (i = 1; i <= n; i++) {
        if (parts[i] == "" || parts[i] == ".") continue
        if (parts[i] == "..") {
            if (depth == 0) return ""
            depth--
            continue
        }
        out[++depth] = parts[i]
    }
    if (depth == 0) return "."
    path = ""
    for (k = 1; k <= depth; k++) path = path (k > 1 ? "/" : "") out[k]
    return path
}

# Returns the run of ``` or ~~~ that opens a fenced block on this line,
# or "".
function fence_marker(line) {
    if (!match(line, /^[ \t]*(```+|~~~+)/)) return ""
    line = substr(line, RSTART, RLENGTH)
    sub(/^[ \t]*/, "", line)
    return line
}

# Returns 1 when the line closes the block that `open` started: the same
# character, a run at least as long, and nothing after it.
function closes_fence(line, open,    marker) {
    marker = fence_marker(line)
    return marker != "" && substr(marker, 1, 1) == substr(open, 1, 1) \
        && length(marker) >= length(open) && line ~ /^[ \t]*(`+|~+)[ \t]*$/
}

# Returns the GitHub anchor of a heading: lowercase, keep letters, digits,
# spaces, '-' and '_' (and non-ASCII bytes), then spaces become '-'.
function slug(text,    s, out, i, c, inner) {
    s = tolower(text)
    while (match(s, /\[[^]]*\]\([^)]*\)/)) {
        inner = substr(s, RSTART + 1, RLENGTH)
        sub(/\]\(.*$/, "", inner)
        s = substr(s, 1, RSTART - 1) inner substr(s, RSTART + RLENGTH)
    }
    out = ""
    for (i = 1; i <= length(s); i++) {
        c = substr(s, i, 1)
        if (index("abcdefghijklmnopqrstuvwxyz0123456789 -_", c) || c > "\177")
            out = out c
    }
    gsub(/ /, "-", out)
    return out
}

function load_anchors(path,    line, fence, marker, text, s, n) {
    if (path in loaded)
        return
    loaded[path] = 1
    fence = ""
    delete seen
    while ((getline line < path) > 0) {
        if (fence != "") { if (closes_fence(line, fence)) fence = ""; continue }
        marker = fence_marker(line)
        if (marker != "") { fence = marker; continue }
        if (line ~ /^#+[ \t]/) {
            text = line
            sub(/^#+[ \t]+/, "", text)
            sub(/[ \t]+#+[ \t]*$/, "", text)
            s = slug(text)
            n = seen[s]++
            if (n > 0) s = s "-" n
            anchors[path, s] = 1
        }
        while (match(line, /(id|name)="[^"]+"/)) {
            s = substr(line, RSTART, RLENGTH)
            sub(/^[a-z]+="/, "", s)
            sub(/"$/, "", s)
            anchors[path, s] = 1
            line = substr(line, RSTART + RLENGTH)
        }
    }
    close(path)
}

function report(line, message) {
    printf "%s:%d: %s\n", FILENAME, line, message
    findings++
}

function dirname(path) {
    if (path !~ /\//) return "."
    sub(/\/[^\/]*$/, "", path)
    return path
}

# Replaces each code span with one word, so it counts as one word and its
# contents are not read as links or sentence ends.
function strip_code(s) {
    gsub(/``[^`]*``/, "CODE", s)
    gsub(/`[^`]*`/, "CODE", s)
    return s
}

function check_link(target, line,    path, anchor, hash) {
    sub(/^</, "", target)
    sub(/>$/, "", target)
    sub(/[ \t]+".*"$/, "", target)
    if (target ~ /^[A-Za-z][A-Za-z0-9+.-]*:/) return
    anchor = ""
    hash = index(target, "#")
    if (hash) {
        anchor = substr(target, hash + 1)
        target = substr(target, 1, hash - 1)
    }
    if (target == "") path = FILENAME
    else if (target ~ /^\//) path = normalize(substr(target, 2))
    else path = normalize(dirname(FILENAME) "/" target)
    if (target != "" && !(path in known)) {
        report(line, "broken link: " target)
        return
    }
    if (anchor != "" && path ~ /\.[Mm][Dd]$/) {
        load_anchors(path)
        if (!((path, anchor) in anchors))
            report(line, "broken anchor: " target "#" anchor)
    }
}

function check_links(s, line,    rest, target) {
    rest = strip_code(s)
    while (match(rest, /\]\([^)]*\)/)) {
        target = substr(rest, RSTART + 2, RLENGTH - 3)
        rest = substr(rest, RSTART + RLENGTH)
        check_link(target, line)
    }
}

function add_prose(s, line) {
    s = strip_code(s)
    # A link counts as its text, not its target.
    while (match(s, /\]\([^)]*\)/))
        s = substr(s, 1, RSTART) substr(s, RSTART + RLENGTH)
    gsub(/[\[\]]/, "", s)
    lines++
    line_offset[lines] = length(unit) + 1
    line_number[lines] = line
    unit = unit s " "
}

function line_at(offset,    k) {
    for (k = lines; k > 1; k--)
        if (line_offset[k] <= offset) return line_number[k]
    return line_number[1]
}

function count_words(s,    parts, n, k, count) {
    n = split(s, parts, /[ \t]+/)
    count = 0
    for (k = 1; k <= n; k++) if (parts[k] != "") count++
    return count
}

function check_sentence(first, last,    words) {
    words = count_words(substr(unit, first, last - first + 1))
    if (words > 25) {
        while (substr(unit, first, 1) == " ") first++
        report(line_at(first), "sentence has " words " words (limit 25)")
    }
}

# Splits the collected paragraph or list item into sentences at '.', '?',
# or '!' followed by a space or the end, and checks each one.
function flush(    i, n, start) {
    if (unit != "" && ste && !verbatim) {
        start = 1
        n = length(unit)
        for (i = 1; i <= n; i++) {
            if (index(".?!", substr(unit, i, 1)) && (i == n || substr(unit, i + 1, 1) == " ")) {
                check_sentence(start, i)
                start = i + 1
            }
        }
        if (start <= n) check_sentence(start, n)
    }
    unit = ""
    lines = 0
}

BEGIN {
    findings = 0
    load_paths()
    # A line that starts with a block-level HTML tag is not prose. Inline
    # tags such as <kbd> or <code> start prose.
    html_block = "^[ \t]*</?(details|summary|div|p|table|thead|tbody|tr|td|th|img|picture|source|br|hr|h[1-6])([ \t/>]|$)"
}

FNR == 1 { load_anchors(FILENAME); fence = ""; comment = 0; verbatim = 0; unit = ""; lines = 0 }

{
    raw = $0
    if (fence != "") { if (closes_fence(raw, fence)) fence = ""; next }
    marker = fence_marker(raw)
    if (marker != "") { flush(); fence = marker; next }

    # The bracket in each pattern keeps this script out of its own term search.
    if (raw ~ /(^|[^A-Za-z0-9_])Q[t]([^A-Za-z0-9_]|$)/) report(FNR, "banned term: other toolkit name")
    if (tolower(raw) ~ /phas[e] [0-9]/) report(FNR, "banned term: phase number")
    if (tolower(raw) ~ /roadma[p]/) report(FNR, "banned term: plan document")
    if (raw ~ /^[ \t]*[-*] \[[ xX]\]([ \t]|$)/) report(FNR, "banned checkbox list item")

    check_links(raw, FNR)
    # A reference definition, such as `[name]: path`, is a link, not prose.
    # A footnote definition (`[^1]: text`) is not a link.
    if (match(raw, /^[ \t]*\[[^]^][^]]*\]:[ \t]*[^ \t]+/)) {
        target = substr(raw, RSTART, RLENGTH)
        sub(/^[^]]*\]:[ \t]*/, "", target)
        check_link(target, FNR)
        flush()
        next
    }

    if (raw ~ /^#/) { flush(); verbatim = 0 }
    if (raw ~ /<!-- check-docs: no-sentence-check \(.+\) -->/) { flush(); verbatim = 1 }

    # Prose for the sentence check: skip HTML comments, headings, tables,
    # and HTML blocks.
    s = raw
    if (comment) {
        if (index(s, "-->") == 0) next
        s = substr(s, index(s, "-->") + 3)
        comment = 0
    }
    while (match(s, /<!--.*-->/)) s = substr(s, 1, RSTART - 1) substr(s, RSTART + RLENGTH)
    if (index(s, "<!--")) { s = substr(s, 1, index(s, "<!--") - 1); comment = 1 }

    if (s ~ /^[ \t]*$/ || s ~ /^#/ || s ~ /^[ \t]*\|/ || s ~ html_block) { flush(); next }
    sub(/^[ \t]*>[ \t]?/, "", s)
    if (s ~ /^[ \t]*([-*+]|[0-9]+[.)])[ \t]+/) {
        flush()
        sub(/^[ \t]*([-*+]|[0-9]+[.)])[ \t]+/, "", s)
    }
    add_prose(s, FNR)
}

END { flush(); exit findings > 0 }
AWK

# All repository paths: tracked files, plus new files that are not ignored,
# so a new document is checked before its first commit. Deleted files are
# left out. core.quotePath=off keeps non-ASCII names unquoted.
if ! listing=$(git -c core.quotePath=off ls-files --cached --others --exclude-standard); then
    echo "check-docs: git ls-files failed" >&2
    exit 2
fi
paths=$(mktemp "${TMPDIR:-/tmp}/check-docs.XXXXXX")
trap 'rm -f "$paths"' EXIT

docs=""
while IFS= read -r file; do
    [ -n "$file" ] && [ -e "$file" ] || continue
    printf '%s\n' "$file" >>"$paths"
    dir=$file
    while [ "${dir%/*}" != "$dir" ]; do
        dir=${dir%/*}
        printf '%s\n' "$dir" >>"$paths"
    done
    case $file in
        docs/specs/*) ;;
        *.md) docs="$docs$file
" ;;
    esac
done <<LIST
$listing
LIST
sort -u -o "$paths" "$paths"

status=0
while IFS= read -r file; do
    [ -n "$file" ] || continue
    case $file in
        CLAUDE.md | .claude/*) ste=0 ;;
        *) ste=1 ;;
    esac
    awk -v ste="$ste" -v paths="$paths" "$program" "$file" || status=1
done <<LIST
$(printf '%s' "$docs" | sort -u)
LIST
exit "$status"
