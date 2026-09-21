/// Keep the global top 40 for every selectable column, not just CPU. The
/// union is bounded to 120 rows before crossing SSH. Its command order is
/// carried through the parser so browser locale collation cannot change which
/// rows belong to the command top 40.
fn select_posix_process_rows_script() -> &'static str {
    r#"procs=$(
  {
    printf '%s\n' "$procs" | LC_ALL=C sort -t'|' -k4,4nr -k1,1n | head -n 40
    printf '%s\n' "$procs" | LC_ALL=C sort -t'|' -k3,3nr -k1,1n | head -n 40
    printf '%s\n' "$procs" | LC_ALL=C sort -t'|' -k6 -k1,1n | head -n 40
  } | LC_ALL=C sort -u | LC_ALL=C sort -t'|' -k6 -k1,1n
)"#
}
