# Reader for the YAML subset of secrets.yaml / secrets.sops.yaml (docs/design/secrets.md §3.1).
# Nix has no fromYAML, and IFD is not an option, so this reads exactly what neo
# and sops write:
#
#   services:            nested maps, two-space indentation (any consistent depth)
#     dufs:
#       password: abc    plain, "double" or 'single' quoted scalars
#     hermes:
#       key: |           block scalars (| > with -/+ chomping), content indented deeper
#         line 1
#   sops: …              the top-level sops metadata subtree is skipped
#
# Anything else (tabs, lists, flow {…}/[…], anchors &/*, tags !, multi-document
# markers after content, keys without a value on the same line that are not
# maps) is an eval error naming the line, so a hand edit can never silently
# drop a secret.
#
#   lib.neo.secrets.readYaml text → nested attrs of strings
{lib, ...}: let
  inherit (builtins) stringLength substring match elemAt length head;

  fail = n: msg: throw "secrets yaml line ${toString n}: ${msg}";

  indentOf = line: stringLength (head (match "( *).*" line));

  # "key: rest" with a bare or quoted key. rest may be empty (map start).
  splitKey = n: body: let
    bare = match "([A-Za-z0-9_.@/-][A-Za-z0-9_.@/:-]*):( (.*))?" body;
    dq = match "\"([^\"\\\\]*)\":( (.*))?" body;
    sq = match "'([^']*)':( (.*))?" body;
    m =
      if bare != null
      then bare
      else if dq != null
      then dq
      else sq;
  in
    if m == null
    then fail n "expected `key: value` or `key:` (lists, flow maps and complex keys are not supported)"
    else {
      key = elemAt m 0;
      rest =
        if elemAt m 2 == null
        then ""
        else elemAt m 2;
    };

  unescapeDouble = n: s: let
    parts = builtins.split "\\\\(.)" s;
    esc = c:
      {
        "n" = "\n";
        "t" = "\t";
        "\"" = "\"";
        "\\" = "\\";
        "/" = "/";
        "r" = "\r";
        "0" = "";
      }
      .${
        c
      }
      or (fail n "unsupported escape \\${c} in double-quoted string");
  in
    lib.concatMapStrings (p:
      if builtins.isList p
      then esc (head p)
      else p)
    parts;

  # Inline scalar → string. Plain scalars lose a trailing ` # comment`.
  scalar = n: rest: let
    t = lib.trim rest;
    dq = match "\"((\\\\.|[^\"\\\\])*)\"( +#.*)?" t;
    sq = match "'(([^']|'')*)'( +#.*)?" t;
    first = substring 0 1 t;
  in
    if dq != null
    then unescapeDouble n (elemAt dq 0)
    else if sq != null
    then builtins.replaceStrings ["''"] ["'"] (elemAt sq 0)
    else if builtins.elem first ["{" "[" "&" "*" "!" "-" "|" ">" "\"" "'" "%" "@" "`"] && t != "-"
    then fail n "unsupported value `${t}` (quote it)"
    else lib.trim (head (match "(([^#]|[^ ]#)*)( #.*)?" t));

  blockHeader = rest: match "([|>])([-+]?)( +#.*)?" (lib.trim rest);

  # Block scalar body: lines (already de-indented) → string per style/chomp.
  blockValue = style: chomp: lines: let
    body =
      if style == "|"
      then lib.concatStringsSep "\n" lines
      else
        # folded: single newlines between non-empty lines become spaces
        lib.concatStringsSep "" (lib.imap0 (i: l:
          if i == 0
          then l
          else if l == "" || elemAt lines (i - 1) == ""
          then "\n" + l
          else " " + l)
        lines);
    dropTrailing = ls:
      if ls != [] && lib.last ls == ""
      then dropTrailing (lib.init ls)
      else ls;
    trimmed = lib.concatStringsSep "\n" (dropTrailing (lib.splitString "\n" body));
  in
    if chomp == "-"
    then trimmed
    else if chomp == "+"
    then body + "\n"
    else trimmed + "\n";

  readYaml = text: let
    lines = lib.splitString "\n" text;
    numbered = lib.imap1 (n: line: {inherit n line;}) lines;

    step = st: {
      n,
      line,
    }: let
      ind = indentOf line;
      content = substring ind (stringLength line) line;
      blank = match " *(#.*)?" line != null;
    in
      # Inside a block scalar: collect deeper-indented (or blank) lines.
      if st.block != null && (blank && match " *" line != null || ind > st.block.parent)
      then
        st
        // {
          block =
            st.block
            // {
              lines = st.block.lines ++ [line];
            };
        }
      else let
        st1 =
          if st.block == null
          then st
          else closeBlock st;
      in
        if blank
        then st1
        else if match ".*\t.*" (head (match "([ \t]*).*" line)) != null
        then fail n "tab in indentation"
        else if ind == 0 && match "(---|\\.\\.\\.)( .*)?" line != null
        then
          if st1.entries == [] && st1.stack == []
          then st1
          else fail n "multiple YAML documents are not supported"
        # Skipping the top-level sops metadata subtree.
        else if st1.skip && ind > 0
        then st1
        else let
          kv = splitKey n content;
          # Pop parents at the same or deeper indentation. Siblings must share
          # one indentation: the first child fixes it for its parent.
          popped = lib.filter (f: f.indent < ind) st1.stack;
          parent =
            if popped == []
            then null
            else lib.last popped;
          stack =
            if parent == null
            then []
            else lib.init popped ++ [(parent // {child = ind;})];
          path = map (f: f.key) stack ++ [kv.key];
          hdr = blockHeader kv.rest;
        in
          if parent == null && ind != 0 || parent != null && parent.child != null && parent.child != ind
          then fail n "bad indentation"
          else if ind == 0 && kv.key == "sops"
          then
            st1
            // {
              skip = true;
              stack = [];
            }
          else if kv.rest == "" || match " *#.*" kv.rest != null
          then
            st1
            // {
              skip = false;
              stack =
                stack
                ++ [
                  {
                    indent = ind;
                    key = kv.key;
                    child = null;
                  }
                ];
              maps = st1.maps ++ [{inherit n path;}];
            }
          else if hdr != null
          then
            st1
            // {
              skip = false;
              inherit stack;
              block = {
                inherit n path;
                parent = ind;
                style = elemAt hdr 0;
                chomp = elemAt hdr 1;
                lines = [];
              };
            }
          else
            st1
            // {
              skip = false;
              inherit stack;
              entries =
                st1.entries
                ++ [
                  {
                    inherit n path;
                    value = scalar n kv.rest;
                  }
                ];
            };

    closeBlock = st: let
      b = st.block;
      nonBlank = lib.filter (l: match " *" l == null) b.lines;
      bodyIndent =
        if nonBlank == []
        then 0
        else lib.foldl' lib.min 1000 (map indentOf nonBlank);
      deindent = l:
        if match " *" l != null
        then ""
        else substring bodyIndent (stringLength l) l;
    in
      st
      // {
        block = null;
        entries =
          st.entries
          ++ [
            {
              inherit (b) n path;
              value = blockValue b.style b.chomp (map deindent b.lines);
            }
          ];
      };

    init = {
      stack = [];
      entries = [];
      maps = [];
      block = null;
      skip = false;
    };
    final0 = lib.foldl' step init numbered;
    final =
      if final0.block == null
      then final0
      else closeBlock final0;

    # A map key with no children would be an empty value in YAML (null). Refuse it.
    emptyMaps = lib.filter (m: !(lib.any (e: lib.take (length m.path) e.path == m.path && length e.path > length m.path) final.entries)) final.maps;

    # Duplicate leaf or a leaf that is also a map → error.
    paths = map (e: lib.concatStringsSep "." e.path) final.entries;
    dup = lib.findFirst (e: lib.count (p: p == lib.concatStringsSep "." e.path) paths > 1) null final.entries;
  in
    if emptyMaps != []
    then fail (head emptyMaps).n "`${lib.last (head emptyMaps).path}:` has no value"
    else if dup != null
    then fail dup.n "duplicate key ${lib.concatStringsSep "." dup.path}"
    else lib.foldl' (acc: e: lib.recursiveUpdate acc (lib.setAttrByPath e.path e.value)) {} final.entries;
in {
  libExtensions.secrets-yaml = {
    neo.secrets.readYaml = readYaml;
  };
}
