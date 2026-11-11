# IPv4 address ranges (CIDR) for assertions: parse, compare, overlap.
{lib, ...}: let
  inherit (lib) toInt splitString elemAt length all;

  pow2 = n:
    if n == 0
    then 1
    else 2 * pow2 (n - 1);

  # "10.213.0.0/16" → { addr = <int>; prefix = 16; }, null when not IPv4 CIDR.
  parse = s: let
    parts = splitString "/" s;
    octets = splitString "." (elemAt parts 0);
    valid =
      length parts
      == 2
      && length octets == 4
      && all (o: builtins.match "[0-9]{1,3}" o != null && toInt o < 256) octets
      && builtins.match "[0-9]{1,2}" (elemAt parts 1) != null
      && toInt (elemAt parts 1) <= 32;
    num = builtins.foldl' (acc: o: acc * 256 + toInt o) 0 octets;
  in
    if valid
    then {
      addr = num;
      prefix = toInt (elemAt parts 1);
    }
    else null;

  # First address of the block, as an integer.
  base = r: r.addr - lib.mod r.addr (pow2 (32 - r.prefix));

  # Two ranges overlap when the shorter prefix contains the other's base.
  overlaps = a: b: let
    ra = parse a;
    rb = parse b;
    size = pow2 (32 - lib.min ra.prefix rb.prefix);
  in
    if ra == null || rb == null
    then false
    else (ra.addr - lib.mod ra.addr size) == (rb.addr - lib.mod rb.addr size);
in {
  libExtensions.cidr = {
    neo.cidr4 = {
      inherit parse overlaps;
      isValid = s: parse s != null;
      # The address part is the block's first address (no host bits set).
      isNetwork = s: let r = parse s; in r != null && base r == r.addr;
    };
  };
}
