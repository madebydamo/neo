# Restart levels at which each trigger runs in this boot (nix/modules/core/restart.nix).
# $new: [the activated system's neo-restart.json]; $base: the recorded levels,
# null on the first activation of a boot. Triggers no longer declared drop out,
# so a service switched off and on again starts at its level then.
$new[0].triggers
| with_entries(.value = (
    if $base == null then .value.level
    else ($base[.key] // (if .value.onEnable then 0 else .value.level end))
    end))
