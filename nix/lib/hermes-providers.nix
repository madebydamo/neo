# Hermes LLM provider catalog, derived from the hermes-agent flake input.
#
# Source of truth (eval-time parse, no IFD; Hermes has no flake output for this):
#   - plugins/model-providers/*/__init__.py — ProviderProfile
#   - hermes_cli/providers.py HERMES_OVERLAYS — ids with no plugin (xai-oauth)
#   - hermes_cli/web_server.py _OAUTH_PROVIDER_CATALOG — dashboard flow + name
#   - hermes_cli/models.py CANONICAL_PROVIDERS labels + _XAI_STATIC_FALLBACK
#
# Plugin auth_type is not the dashboard flow (openai-codex / minimax-oauth
# declare oauth_external but the UI is device_code). Read the catalog, do not
# keep a Neo id table.
#
# env var null = no named API-key env. First env var that is not *_BASE_URL
# is the Neo llm.apiKey target. Empty plugin base_url (custom) still accepts
# llm.apiKey → model.api_key and needs a Base URL field.
{
  inputs,
  lib,
  ...
}: let
  hermesSrc = inputs.hermes-agent.outPath or inputs.hermes-agent.sourceInfo.outPath;
  providersDir = hermesSrc + "/plugins/model-providers";
  overlaysPy = hermesSrc + "/hermes_cli/providers.py";
  modelsPy = hermesSrc + "/hermes_cli/models.py";
  webServerPy = hermesSrc + "/hermes_cli/web_server.py";

  isList = builtins.isList;
  captures = regex: str:
    builtins.concatLists (builtins.filter isList (builtins.split regex str));
  firstCapture = regex: str: let
    cs = captures regex str;
  in
    if cs == []
    then null
    else builtins.head cs;

  credFromVars = vars:
    lib.findFirst (e: !(lib.hasSuffix "_BASE_URL" e)) null vars;

  varsFromTuple = inner: captures ''"([A-Z][A-Z0-9_]*)"'' inner;

  # env_vars=("FOO", "BAR") or env_vars=_CONST with _CONST = ("FOO", ...) in-file.
  credFromEnvExpr = content: expr: let
    trimmed = lib.trim expr;
    ident = builtins.match "([A-Za-z_][A-Za-z0-9_]*)" trimmed;
  in
    if ident != null
    then let
      inner = firstCapture "${builtins.head ident}[[:space:]]*=[[:space:]]*\\(([^)]*)\\)" content;
    in
      credFromVars (varsFromTuple (
        if inner == null
        then ""
        else inner
      ))
    else credFromVars (varsFromTuple trimmed);

  modelsFromTuple = inner: captures ''"([^"]+)"'' inner;

  parseFile = content: let
    parts = builtins.split "[[:alnum:]_]*Profile[[:space:]]*\\(" content;
    blocks = lib.drop 1 (builtins.filter builtins.isString parts);
    parseBlock = block: let
      name = firstCapture ''name[[:space:]]*=[[:space:]]*"([a-z0-9][a-z0-9-]*)"'' block;
      envExpr = firstCapture "env_vars[[:space:]]*=[[:space:]]*(\\([^)]*\\)|[A-Za-z_][A-Za-z0-9_]*)" block;
      authType = firstCapture ''auth_type[[:space:]]*=[[:space:]]*"([a-z_]+)"'' block;
      displayName = firstCapture ''display_name[[:space:]]*=[[:space:]]*"([^"]*)"'' block;
      modelsInner = firstCapture "fallback_models[[:space:]]*=[[:space:]]*\\(([^)]*)\\)" block;
      baseUrl = firstCapture ''base_url[[:space:]]*=[[:space:]]*"([^"]*)"'' block;
    in
      if name == null
      then null
      else {
        inherit name;
        env =
          if envExpr == null
          then null
          else credFromEnvExpr content envExpr;
        authType =
          if authType == null
          then "api_key"
          else authType;
        inherit displayName;
        models =
          if modelsInner == null
          then []
          else modelsFromTuple modelsInner;
        needsBaseUrl = baseUrl == "";
      };
  in
    builtins.filter (x: x != null) (map parseBlock blocks);

  dirents = builtins.readDir providersDir;
  files =
    lib.mapAttrsToList (
      name: type:
        if type == "directory" && builtins.pathExists (providersDir + "/${name}/__init__.py")
        then builtins.readFile (providersDir + "/${name}/__init__.py")
        else null
    )
    dirents;

  parsed = lib.concatMap parseFile (builtins.filter (x: x != null) files);

  # Walk split() output of a capturing regex: [str, [cap], str, [cap], ...].
  parseCapturedBlocks = parseBody: parts: let
    go = xs:
      if xs == []
      then []
      else if !(builtins.isList (builtins.head xs))
      then go (builtins.tail xs)
      else let
        id = builtins.head (builtins.head xs);
        rest = builtins.tail xs;
        body =
          if rest == []
          then ""
          else if builtins.isString (builtins.head rest)
          then builtins.head rest
          else "";
      in
        [(parseBody id body)] ++ go rest;
  in
    go parts;

  parseOverlays = content:
    parseCapturedBlocks (
      id: body: {
        name = id;
        env = let
          envInner = firstCapture "extra_env_vars[[:space:]]*=[[:space:]]*\\(([^)]*)\\)" body;
        in
          if envInner == null
          then null
          else credFromVars (varsFromTuple envInner);
        authType = let
          authType = firstCapture ''auth_type[[:space:]]*=[[:space:]]*"([a-z_]+)"'' body;
        in
          if authType == null
          then "api_key"
          else authType;
        displayName = null;
        models = [];
        needsBaseUrl = false;
      }
    ) (builtins.split "\"([a-z0-9][a-z0-9-]*)\"[[:space:]]*:[[:space:]]*HermesOverlay[[:space:]]*\\(" content);

  overlayProfiles =
    if builtins.pathExists overlaysPy
    then parseOverlays (builtins.readFile overlaysPy)
    else [];

  modelsContent =
    if builtins.pathExists modelsPy
    then builtins.readFile modelsPy
    else "";

  canonicalLabels = let
    pairs = builtins.filter isList (
      builtins.split ''ProviderEntry[[:space:]]*\([[:space:]]*"([a-z0-9-]+)"[[:space:]]*,[[:space:]]*"([^"]+)"'' modelsContent
    );
    fromEntries = lib.listToAttrs (
      map (p: {
        name = builtins.elemAt p 0;
        value = builtins.elemAt p 1;
      }) (builtins.filter (p: builtins.length p >= 2) pairs)
    );
    customLabel = firstCapture ''_PROVIDER_LABELS[[]"custom"[]][[:space:]]*=[[:space:]]*"([^"]+)"'' modelsContent;
  in
    fromEntries
    // lib.optionalAttrs (customLabel != null) {custom = customLabel;};

  # xAI plugin has no fallback_models; Hermes keeps the floor here.
  xaiFallbackModels = let
    inner = firstCapture "_XAI_STATIC_FALLBACK[[:space:]]*:[[:space:]]*list[[]str[]][[:space:]]*=[[:space:]]*[[]([^]]*)[]]" modelsContent;
  in
    if inner == null
    then []
    else modelsFromTuple inner;

  # Dashboard cards: id / name / flow. Slice between the first two mentions
  # of the marker so we do not scan the rest of web_server.py.
  oauthCatalogRows = let
    content =
      if builtins.pathExists webServerPy
      then builtins.readFile webServerPy
      else "";
    # split() with no capture groups inserts [] for each match; keep the
    # string between the first two mentions (the catalog tuple).
    strings = builtins.filter builtins.isString (builtins.split "_OAUTH_PROVIDER_CATALOG" content);
    region =
      if builtins.length strings < 2
      then ""
      else builtins.elemAt strings 1;
  in
    parseCapturedBlocks (
      id: body: {
        name = id;
        flow = firstCapture ''"flow"[[:space:]]*:[[:space:]]*"(pkce|device_code|external)"'' body;
        displayName = firstCapture ''"name"[[:space:]]*:[[:space:]]*"([^"]+)"'' body;
      }
    ) (builtins.split ''"id"[[:space:]]*:[[:space:]]*"([a-z0-9][a-z0-9-]*)"'' region);

  oauthFlows = lib.listToAttrs (
    map (e: {
      name = e.name;
      value = e.flow;
    }) (builtins.filter (e: e.flow != null) oauthCatalogRows)
  );
  oauthNames = lib.listToAttrs (
    map (e: {
      name = e.name;
      value = e.displayName;
    }) (builtins.filter (e: e.displayName != null && e.displayName != "") oauthCatalogRows)
  );

  # Overlay-only OAuth ids (no plugin directory). Do not add catalog-only
  # synthetic rows such as claude-code — those are not model.provider values.
  overlayOauthExtras =
    builtins.filter (
      p:
        !(builtins.any (q: q.name == p.name) parsed)
        && p.authType != "virtual"
        && (
          lib.hasPrefix "oauth_" p.authType
          || builtins.hasAttr p.name oauthFlows
        )
    )
    overlayProfiles;

  deriveFlow = p: let
    catalogFlow = oauthFlows.${p.name} or null;
  in
    if catalogFlow != null
    then catalogFlow
    else if p.authType == "oauth_device_code"
    then "device_code"
    else if builtins.elem p.authType ["oauth_external" "copilot" "external_process"]
    then "external"
    else null;

  toEntry = p: let
    id = p.name;
    env = p.env;
    oauthFlow = deriveFlow p;
    needsBaseUrl = p.needsBaseUrl or false;
    hasApiKey = (env != null && env != "") || needsBaseUrl;
    hasOauth = oauthFlow != null;
    label =
      if builtins.hasAttr id canonicalLabels
      then canonicalLabels.${id}
      else if builtins.hasAttr id oauthNames
      then oauthNames.${id}
      else if p.displayName != null && p.displayName != ""
      then p.displayName
      else id;
    pluginModels = p.models or [];
    models =
      if pluginModels != []
      then pluginModels
      else if builtins.elem id ["xai" "xai-oauth"]
      then xaiFallbackModels
      else [];
  in {
    inherit id label env;
    envVar = env;
    authType = p.authType;
    inherit hasApiKey hasOauth oauthFlow needsBaseUrl models;
  };

  allProfiles = parsed ++ overlayOauthExtras;

  catalog = map toEntry allProfiles;
  catalogById = lib.listToAttrs (
    map (e: {
      name = e.id;
      value = e;
    })
    catalog
  );

  envVars = lib.mapAttrs (_: e: e.envVar) catalogById;

  nonEmpty = v: v != null && v != "";
in
  assert envVars ? xai && envVars.xai == "XAI_API_KEY";
  assert envVars ? anthropic && envVars.anthropic == "ANTHROPIC_API_KEY";
  assert envVars ? openrouter && envVars.openrouter == "OPENROUTER_API_KEY";
  assert catalogById ? openai-codex && catalogById.openai-codex.hasOauth;
  assert catalogById ? xai-oauth && catalogById.xai-oauth.hasOauth && !catalogById.xai-oauth.hasApiKey;
  assert catalogById ? anthropic && catalogById.anthropic.hasApiKey && catalogById.anthropic.hasOauth;
  assert catalogById.openai-codex.oauthFlow == "device_code";
  assert catalogById.xai-oauth.oauthFlow == "device_code";
  assert catalogById.anthropic.oauthFlow == "pkce";
  assert catalogById ? custom && catalogById.custom.hasApiKey && catalogById.custom.needsBaseUrl && catalogById.custom.envVar == null;
  assert builtins.length (lib.attrNames envVars) > 20; {
    libExtensions.hermesProviders = {
      neo = {
        hermesProviderEnvVars = envVars;
        hermesProviderIds = lib.sort (a: b: a < b) (lib.attrNames envVars);
        hermesProviderCatalog = lib.sort (a: b: a.id < b.id) catalog;
        hermesProviderById = catalogById;

        mkHermesLlmEnv = {
          provider ? null,
          apiKey ? null,
        }: let
          envName =
            if nonEmpty provider && builtins.hasAttr provider envVars
            then envVars.${provider}
            else null;
        in
          lib.optionalAttrs (envName != null && nonEmpty apiKey) {
            ${envName} = apiKey;
          };

        # Wrapper that execs oauth.py with HERMES_PYTHON from the hermes binary.
        mkNeoHermesAuth = pkgs: pythonScript:
          pkgs.writeShellApplication {
            name = "neo-hermes-auth";
            runtimeInputs = [pkgs.coreutils pkgs.gnused];
            text = ''
              set -euo pipefail
              find_hermes() {
                if command -v hermes >/dev/null 2>&1; then
                  command -v hermes
                elif [[ -x /run/current-system/sw/bin/hermes ]]; then
                  echo /run/current-system/sw/bin/hermes
                else
                  echo ""
                fi
              }
              hermes_bin=$(find_hermes)
              if [[ -z "$hermes_bin" ]]; then
                echo '{"ok":false,"error":"Hermes is not installed on this host. Activate with Hermes enabled, then retry OAuth from the live Neo UI."}'
                exit 2
              fi
              py=$(sed -n "s/^export HERMES_PYTHON='\\(.*\\)'/\\1/p" "$hermes_bin" | head -n1)
              if [[ -z "$py" || ! -x "$py" ]]; then
                echo '{"ok":false,"error":"Could not find HERMES_PYTHON from the hermes wrapper."}'
                exit 2
              fi
              exec "$py" "${pythonScript}" "$@"
            '';
          };
      };
    };
  }
