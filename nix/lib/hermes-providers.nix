# Hermes LLM provider catalog, derived from the hermes-agent flake input.
#
# Source of truth:
#   - plugins/model-providers/*/__init__.py (ProviderProfile name= + env_vars=
#     + auth_type= + display_name= + fallback_models=)
#   - hermes_cli/providers.py HERMES_OVERLAYS (ids with no plugin, e.g. xai-oauth)
#   - hermes_cli/auth_commands.py _OAUTH_CAPABLE_PROVIDERS
#   - hermes_cli/models.py CANONICAL_PROVIDERS labels + _XAI_STATIC_FALLBACK
# Hermes has no flake output for this (configKeys is config.yaml leaves only).
# Parsed at eval time from the input source — no IFD.
#
# env var null = no named API-key env (OAuth / AWS SDK / Vertex ADC / keyless).
# First env var that is not *_BASE_URL is the Neo llm.apiKey target.
# `custom` still accepts llm.apiKey (written to model.api_key, not an env var).
#
# oauthFlow is what the Neo providerAuth widget uses:
#   device_code | pkce | external | null
# Plugin auth_type is not enough (openai-codex and minimax-oauth declare
# oauth_external but the dashboard flow is device_code).
# SuperGrok is a Hermes overlay (`xai-oauth`), not a plugin — same as Hermes's
# own registry. Anthropic and openai-codex are plugins; we parse both sources.
{
  inputs,
  lib,
  ...
}: let
  hermesSrc = inputs.hermes-agent.outPath or inputs.hermes-agent.sourceInfo.outPath;
  providersDir = hermesSrc + "/plugins/model-providers";
  authCommands = hermesSrc + "/hermes_cli/auth_commands.py";
  overlaysPy = hermesSrc + "/hermes_cli/providers.py";
  modelsPy = hermesSrc + "/hermes_cli/models.py";

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
        displayName = displayName;
        models =
          if modelsInner == null
          then []
          else modelsFromTuple modelsInner;
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

  # Hermes overlays (hermes_cli/providers.py). SuperGrok (`xai-oauth`) is an
  # overlay, not a plugin — Anthropic/Codex are plugins. Merge overlay-only
  # OAuth ids so Neo does not hand-maintain that list.
  parseOverlays = content: let
    parts = builtins.split "\"([a-z0-9][a-z0-9-]*)\"[[:space:]]*:[[:space:]]*HermesOverlay[[:space:]]*\\(" content;
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
        authType = firstCapture ''auth_type[[:space:]]*=[[:space:]]*"([a-z_]+)"'' body;
        envInner = firstCapture "extra_env_vars[[:space:]]*=[[:space:]]*\\(([^)]*)\\)" body;
        keyless = builtins.match ".*keyless[[:space:]]*=[[:space:]]*True.*" body != null;
      in
        [
          {
            name = id;
            env =
              if envInner == null
              then null
              else credFromVars (varsFromTuple envInner);
            authType =
              if authType == null
              then "api_key"
              else authType;
            displayName = null;
            models = [];
            inherit keyless;
          }
        ]
        ++ go rest;
  in
    go parts;

  overlayProfiles =
    if builtins.pathExists overlaysPy
    then parseOverlays (builtins.readFile overlaysPy)
    else [];

  canonicalLabels = let
    content =
      if builtins.pathExists modelsPy
      then builtins.readFile modelsPy
      else "";
    pairs = builtins.filter isList (
      builtins.split ''ProviderEntry[[:space:]]*\([[:space:]]*"([a-z0-9-]+)"[[:space:]]*,[[:space:]]*"([^"]+)"'' content
    );
  in
    lib.listToAttrs (
      map (p: {
        name = builtins.elemAt p 0;
        value = builtins.elemAt p 1;
      }) (builtins.filter (p: builtins.length p >= 2) pairs)
    );

  xaiFallbackModels = let
    content =
      if builtins.pathExists modelsPy
      then builtins.readFile modelsPy
      else "";
    inner = firstCapture "_XAI_STATIC_FALLBACK[[:space:]]*:[[:space:]]*list[[]str[]][[:space:]]*=[[:space:]]*[[]([^]]*)[]]" content;
  in
    if inner == null
    then []
    else modelsFromTuple inner;

  oauthCapable = let
    content =
      if builtins.pathExists authCommands
      then builtins.readFile authCommands
      else "";
    inner = firstCapture "_OAUTH_CAPABLE_PROVIDERS[[:space:]]*=[[:space:]]*[{]([^}]*)[}]" content;
  in
    if inner == null
    then ["anthropic" "nous" "openai-codex" "xai-oauth" "qwen-oauth" "minimax-oauth"]
    else captures ''"([a-z0-9][a-z0-9-]*)"'' inner;

  overlayOauthExtras =
    builtins.filter (
      p:
        !(builtins.any (q: q.name == p.name) parsed)
        && (
          builtins.elem p.name oauthCapable
          || lib.hasPrefix "oauth_" p.authType
        )
        && p.authType != "virtual"
    )
    overlayProfiles;

  # Last-resort if overlay parse misses SuperGrok (Hermes still lists it in
  # _OAUTH_CAPABLE_PROVIDERS). Prefer the parsed overlay row.
  extras =
    if builtins.any (p: p.name == "xai-oauth") overlayOauthExtras
    then overlayOauthExtras
    else
      overlayOauthExtras
      ++ [
        {
          name = "xai-oauth";
          env = null;
          authType = "oauth_external";
          displayName = canonicalLabels."xai-oauth" or "xAI Grok OAuth (SuperGrok / Premium+)";
          models = xaiFallbackModels;
        }
      ];

  # Dashboard catalog flows. Plugin auth_type is wrong for several of these
  # (openai-codex / minimax-oauth declare oauth_external but use device_code).
  oauthFlowOverrides = {
    anthropic = "pkce";
    nous = "device_code";
    openai-codex = "device_code";
    xai-oauth = "device_code";
    qwen-oauth = "external";
    minimax-oauth = "device_code";
  };

  labelOverrides = {
    openai-codex = "ChatGPT / Codex subscription";
    anthropic = "Anthropic (API key or Claude OAuth)";
    nous = "Nous Portal";
  };

  deriveFlow = p: let
    id = p.name;
    override = oauthFlowOverrides.${id} or null;
    inCapable = builtins.elem id oauthCapable;
  in
    if override != null
    then override
    else if p.authType == "oauth_device_code"
    then "device_code"
    else if builtins.elem p.authType ["oauth_external" "copilot" "external_process"]
    then "external"
    else if inCapable
    then "device_code"
    else null;

  toEntry = p: let
    id = p.name;
    env = p.env;
    oauthFlow = deriveFlow p;
    needsBaseUrl = id == "custom";
    # custom has env_vars=() but still takes llm.apiKey → model.api_key.
    hasApiKey = (env != null && env != "") || needsBaseUrl;
    hasOauth = oauthFlow != null;
    label =
      if builtins.hasAttr id labelOverrides
      then labelOverrides.${id}
      else if builtins.hasAttr id canonicalLabels
      then canonicalLabels.${id}
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

  allProfiles =
    parsed
    ++ builtins.filter (e: !(builtins.any (p: p.name == e.name) parsed)) extras;

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
