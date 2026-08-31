# Unit checks for lib.neo local/split-horizon DNS helpers.
{...}: {
  perSystem = {
    pkgs,
    lib,
    ...
  }: let
    dns = (import ./dns.nix {inherit lib;}).libExtensions.dns.neo;

    expectEq = name: actual: expected: let
      ok = actual == expected;
    in
      if ok
      then ""
      else ''
        echo "FAIL ${name}" >&2
        echo "  expected: ${lib.generators.toPretty {} expected}" >&2
        echo "  actual:   ${lib.generators.toPretty {} actual}" >&2
        fail=1
      '';

    namesSub = dns.localDnsNames {
      domain = "damo4mf20.ch";
      onlySubdomains = true;
      subdomains = ["app" "swag"];
      customDomains = ["www.example.com"];
      proxyPassDomains = ["octo.example.com"];
    };
    namesApex = dns.localDnsNames {
      domain = "damo4mf20.ch";
      onlySubdomains = false;
      subdomains = ["app"];
      customDomains = [];
      proxyPassDomains = [];
    };
    namesNoDomain = dns.localDnsNames {
      domain = null;
      onlySubdomains = true;
      subdomains = ["app"];
      customDomains = ["www.example.com"];
      proxyPassDomains = ["octo.example.com"];
    };
    namesDedupe = dns.localDnsNames {
      domain = "example.com";
      onlySubdomains = false;
      subdomains = ["app"];
      customDomains = ["app.example.com" "example.com"];
      proxyPassDomains = [];
    };

    portsOpen = dns.piholeDnsPublishPorts {
      splitDnsActive = false;
      localIP = "10.0.0.5";
    };
    portsPinned = dns.piholeDnsPublishPorts {
      splitDnsActive = true;
      localIP = "10.0.0.5";
    };

    splitOn = dns.splitDnsActive {
      neo.services.tailscale = {
        enabled = true;
        splitDns = true;
      };
    };
    splitOffFlag = dns.splitDnsActive {
      neo.services.tailscale = {
        enabled = true;
        splitDns = false;
      };
    };
    splitTailscaleOff = dns.splitDnsActive {
      neo.services.tailscale = {
        enabled = false;
        splitDns = true;
      };
    };

    body = lib.concatStrings [
      (expectEq "names/subdomains-only" namesSub [
        "app.damo4mf20.ch"
        "swag.damo4mf20.ch"
        "www.example.com"
        "octo.example.com"
      ])
      (expectEq "names/include-apex" namesApex [
        "app.damo4mf20.ch"
        "damo4mf20.ch"
      ])
      (expectEq "names/null-domain" namesNoDomain [
        "www.example.com"
        "octo.example.com"
      ])
      (expectEq "names/unique" namesDedupe [
        "app.example.com"
        "example.com"
      ])
      (expectEq "pihole-ports/unrestricted" portsOpen [
        "53:53/tcp"
        "53:53/udp"
      ])
      (expectEq "pihole-ports/pinned-to-localIP" portsPinned [
        "10.0.0.5:53:53/tcp"
        "10.0.0.5:53:53/udp"
      ])
      (expectEq "splitDnsActive/on" splitOn true)
      (expectEq "splitDnsActive/flag-off" splitOffFlag false)
      (expectEq "splitDnsActive/tailscale-off" splitTailscaleOff false)
    ];
  in {
    checks.local-dns-names = pkgs.runCommand "local-dns-names" {} ''
      set -euo pipefail
      fail=0
      ${body}
      if [ "$fail" -ne 0 ]; then
        exit 1
      fi

      pihole=${../services/pihole/default.nix}
      if ! grep -q 'piholeDnsPublishPorts' "$pihole"; then
        echo "FAIL pihole must publish DNS ports via piholeDnsPublishPorts" >&2
        exit 1
      fi
      if grep -qE '"53:53/(tcp|udp)"' "$pihole"; then
        echo "FAIL pihole must not hardcode wildcard 53 publish ports" >&2
        exit 1
      fi

      touch "$out"
    '';
  };
}
