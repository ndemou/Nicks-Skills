# Source with: source <(tat completions bash)
_tat_completion() {
    local candidate description escaped
    local -a words=("${COMP_WORDS[@]:0:COMP_CWORD}" "${COMP_WORDS[COMP_CWORD]-}")
    COMPREPLY=()
    while IFS=$'\t' read -r candidate description; do
        [[ -n $candidate ]] || continue
        printf -v escaped '%q' "$candidate"
        COMPREPLY+=("$escaped")
    done < <(tat __complete -- "${words[@]}" 2>/dev/null)
}
complete -o default -o bashdefault -F _tat_completion tat
