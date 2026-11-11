With [NetBird server](service:netbird-server), one of your servers runs a private
network of its own. Your phones, laptops and other Neo servers join it with
[NetBird](service:netbird) and reach each other directly and encrypted, wherever
they are. Nobody else runs any part of it, and no outside account is needed.

Once every server is in the network, you can close your apps to the internet
and still open them from all your devices.

## Where to run it

Devices find a direct way to each other with the help of the network's server.
For that it needs a public address that answers on a UDP port, which a tunnel
cannot pass on.

- **A rented server (VPS) with a public address:** the best choice. If it
  already runs Streamproxy for your home servers, run NetBird server there too.
- **At home, with ports 443 and 3478 (UDP) forwarded on the router:** works as
  well.
- **At home behind a tunnel ([Rathole](service:rathole)) only:** devices can
  join, but every connection goes through the relay and is slow. Neo warns
  about this.

## 1. Switch on NetBird server

On the machine that will run the network, open
[NetBird server](service:netbird-server) and switch it on. Fill in:

- [Administrator e-mail](option:netbird-server.ownerEmail): your e-mail address,
  used to sign in to the dashboard.
- [Administrator password](option:netbird-server.ownerPassword),
  [Relay secret](option:netbird-server.relaySecret),
  [Database key](option:netbird-server.dataKey) and
  [Login session key](option:netbird-server.sessionKey): press **Generate** on
  each. Keep the administrator password somewhere safe.

The network's address is `https://netbird.{{domain}}`. Keep **Internet** ticked
under **Reachable from**: devices away from home must reach it.

## 2. Add this server to its own network

Also switch on [NetBird](service:netbird) on the same machine, and turn on
[App names over NetBird](option:netbird.appNames) if devices should open this
server's apps by name. Leave the setup key and network address empty.

Apply the change. On the first start Neo creates the administrator, sets the
network's address range, adds this server and creates the DNS zone for its app
names.

> [!warning]
> If the server sits behind a firewall of the hosting company, allow incoming
> **UDP 3478** there as well.

> [!check] The network is up
> ```neo-check
> systemctl is-active docker-netbird-server docker-netbird-dashboard netbird-server-setup netbird && netbird status | head -6
> ```

## 3. Sign in to the dashboard

Open `https://netbird.{{domain}}` and sign in with the administrator e-mail and
password. Under **Peers** you see this server.

## 4. Add your other servers

In the dashboard open **Setup Keys** and create one key per server (one-off),
or one reusable key for all of them. On each other Neo server, follow
[Keep your app traffic private with NetBird](guide:private-traffic-netbird):
paste the key, and set the network address to `https://netbird.{{domain}}`.

Each of them gets its own DNS zone for its domain, for example
`dev.example.com` and `prod.example.com`, pointing at its own NetBird address.

## 5. Add your phones and laptops

Create a user for each person under **Team → Users**, or use the administrator
login yourself. Install the NetBird app, enter `https://netbird.{{domain}}` as
the server address, and sign in.

## 6. Decide who may reach what

By default every device in the network reaches every other one, including
ports on your servers that are closed to the home network. Under
**Access Control**, narrow it: for example, phones may reach the servers on
port 443 only.

> [!tip]
> Exit nodes and access to a whole home network are set under **Network
> Routes** in the dashboard. On the server that should offer them, turn on
> [Route for the network](option:netbird.routing).
