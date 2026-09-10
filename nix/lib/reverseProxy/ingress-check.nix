# Unit checks for per-service ingress maps and SWAG wiring.
{...}: {
  perSystem = {
    pkgs,
    lib,
    ...
  }: let
    ingress = (import ./ingress.nix {inherit lib;}).libExtensions.ingress.neo;

    expectTrue = name: cond:
      if cond
      then ""
      else ''
        echo "FAIL ${name}" >&2
        fail=1
      '';

    localOnly = {
      pastebin = {
        subdomain = "pastebin";
        customDomains = ["pastes.example.com"];
        ingress = ["local"];
      };
    };
    allThree = {
      neo = {
        subdomain = "neo";
        customDomains = [];
        ingress = ["local" "tailscale" "web"];
      };
    };
    webAndTailscale = {
      vault = {
        subdomain = "vaultwarden";
        ingress = ["tailscale" "web"];
      };
    };
    mixed = localOnly // allThree // webAndTailscale;

    localConf = ingress.mkIngressMapsConf {services = localOnly;};
    allConf = ingress.mkIngressMapsConf {services = allThree;};
    mixedConf = ingress.mkIngressMapsConf {services = mixed;};
    altPortConf = ingress.mkIngressMapsConf {
      services = allThree;
      proxyProtocolPort = 9982;
    };

    body = lib.concatStrings [
      (expectTrue "variants" (ingress.ingressVariants == ["local" "tailscale" "web"]))
      (expectTrue "access-returns-404" (lib.hasInfix "return 404;" ingress.ingressAccessConf))
      (expectTrue "access-exempts-error-assets" (lib.hasInfix "/_neo404/" ingress.ingressAccessConf))
      (expectTrue "access-class-web" (lib.hasInfix "$ingress_allow_web" ingress.ingressAccessConf))
      (expectTrue "pp-port-default" (lib.hasInfix "8443 1;" localConf))
      (expectTrue "pp-port-override" (lib.hasInfix "9982 1;" altPortConf))
      (expectTrue "class-pp-is-web" (lib.hasInfix ''"1:0:0" web;'' localConf))
      (expectTrue "class-lan-is-local" (lib.hasInfix ''"0:1:0" local;'' localConf))
      (expectTrue "class-ts-is-tailscale" (lib.hasInfix ''"0:0:1" tailscale;'' localConf))
      (expectTrue "class-public-443-is-web" (lib.hasInfix ''"0:0:0" web;'' localConf))
      (expectTrue "tailscale-cgnat" (lib.hasInfix "100.64.0.0/10 1;" localConf))
      (expectTrue "local-only-denies-web-sub" (lib.hasInfix "~*^pastebin(\\.|$) 0;" localConf))
      (expectTrue "local-only-denies-web-custom" (lib.hasInfix "pastes.example.com 0;" localConf))
      (expectTrue "all-three-no-host-deny" (!lib.hasInfix "~*^neo(\\.|$)" allConf))
      (expectTrue "mixed-vault-denies-local" (
        lib.hasInfix "~*^vaultwarden(\\.|$) 0;" mixedConf
        && lib.hasInfix "~*^pastebin(\\.|$) 0;" mixedConf
      ))
    ];
  in {
    checks.ingress-posture = pkgs.runCommand "ingress-posture" {} ''
      set -euo pipefail
      fail=0
      ${body}
      if [ "$fail" -ne 0 ]; then
        exit 1
      fi

      listen=${./listen.nix}
      swag=${../../services/swag/default.nix}
      option=${./reverseProxy.nix}
      patcher=${../../services/swag/swag-patcher.sh}

      if ! grep -q 'ingress-access.conf' "$listen"; then
        echo "FAIL listenHttpsConf must include ingress-access.conf" >&2
        exit 1
      fi
      if ! grep -q 'mkIngressMapsConf' "$swag"; then
        echo "FAIL swag must materialize ingress-maps.conf via mkIngressMapsConf" >&2
        exit 1
      fi
      if ! grep -q 'ingressAccessConf' "$swag"; then
        echo "FAIL swag must materialize ingress-access.conf" >&2
        exit 1
      fi
      if ! grep -q 'ingress-maps.conf' "$patcher"; then
        echo "FAIL swag-patcher must include ingress-maps.conf in nginx.conf" >&2
        exit 1
      fi
      if ! grep -q 'ingress' "$option"; then
        echo "FAIL mkReverseProxyOptions must declare ingress" >&2
        exit 1
      fi

      touch "$out"
    '';
  };
}
