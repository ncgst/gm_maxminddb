# 🌍 gm_maxminddb_geoip

`gm_maxminddb_geoip` is a Garry's Mod binary module for reading MaxMind DB `.mmdb` files and returning GeoIP data as Lua tables.

In other words:

```text
IP address goes in, GeoIP / ASN data comes out.
```

This updated version supports loading **GeoLite2 ASN** and **GeoLite2 Country** at the same time, and adds a convenience function:

```lua
maxminddb.ip_info(ipAddress, lang)
```

That function returns a combined table containing:

- ASN number
- ASN network range / CIDR
- ASN organisation / provider name
- country code
- country name
- registered country
- continent

> This module does **not** detect VPNs, proxies, hosting providers, or residential proxies by itself when you only install GeoLite2 ASN + GeoLite2 Country. For that, you need MaxMind Anonymous IP, GeoIP2 ISP/Anonymous IP, or another IP risk provider.

---

# Installation

## Downloading the module

Run this in your server console to see which binary module filename your server needs:

```lua
lua_run print("gmsv_maxminddb_geoip_" .. ((system.IsLinux() and "linux" .. (jit.arch == "x86" and "" or "64")) or (system.IsWindows() and "win" .. (jit.arch == "x86" and "32" or "64")) or "UNSUPPORTED") .. ".dll")
```

Download the matching binary from your releases page, then place it in:

```text
garrysmod/lua/bin/
```

Create the folder if it does not exist.

---

# Downloading the MaxMind databases

Create a MaxMind account and download the **MMDB** versions of the databases you need.

For the new combined `ip_info()` function, you need both of these files:

```text
GeoLite2-ASN.mmdb
GeoLite2-Country.mmdb
```

Do **not** download the CSV versions for this module.

## Required database paths

Place the files in either `garrysmod/`:

```text
garrysmod/GeoLite2-ASN.mmdb
garrysmod/GeoLite2-Country.mmdb
```

or in `garrysmod/data/`:

```text
garrysmod/data/GeoLite2-ASN.mmdb
garrysmod/data/GeoLite2-Country.mmdb
```

The module checks `garrysmod/` first, then `garrysmod/data/`.

## Optional legacy / full database path

The old single-database paths are still supported as an optional fallback for advanced raw queries:

```text
garrysmod/maxminddb.mmdb
garrysmod/data/maxminddb.dat
```

These are only needed if you want to use `maxminddb.query()` with records other than `Asn` and `Country`, such as:

```lua
maxminddb.records.City
maxminddb.records.Isp
maxminddb.records.AnonymousIp
maxminddb.records.ConnectionType
maxminddb.records.Domain
maxminddb.records.DensityIncome
```

If you only use `maxminddb.ip_info()`, `maxminddb.country()`, `maxminddb.query(ip, maxminddb.records.Asn)`, or `maxminddb.query(ip, maxminddb.records.Country)`, you only need:

```text
GeoLite2-ASN.mmdb
GeoLite2-Country.mmdb
```

---

# Which database do I need?

| Need | Database |
|---|---|
| ASN number, ASN organisation, CIDR range | `GeoLite2-ASN.mmdb` |
| Country code, country name, registered country, continent | `GeoLite2-Country.mmdb` |
| City, postal code, subdivisions, coordinates | `GeoLite2-City.mmdb` or paid GeoIP2 City |
| ISP name / organisation beyond ASN organisation | Paid GeoIP2 ISP database |
| VPN / proxy / hosting / Tor detection | Anonymous IP database or another IP risk provider |

---

# Loading the module

```lua
require("maxminddb_geoip")

print(maxminddb.VERSION)
```

The module automatically loads the databases when required.

To reload the databases from disk:

```lua
-- maxminddb.refresh() -> success: boolean [, error: string]
local ok, err = maxminddb.refresh()

if not ok then
    error(err)
end
```

Use `refresh()` after replacing database files while the server is running.

---

# Complete Lua example

Save this as:

```text
lua/autorun/server/sv_maxminddb_example.lua
```

```lua
require("maxminddb_geoip")

print("[MaxMindDB] Module version:", maxminddb.VERSION)

local ok, refreshErr = maxminddb.refresh()
if ok == false then
    ErrorNoHalt("[MaxMindDB] Failed to load databases: " .. tostring(refreshErr) .. "\n")
    return
end

-- Player:IPAddress() usually returns "ip:port" on Garry's Mod servers.
-- This helper strips the port for IPv4 and handles bracketed IPv6 like "[2001:db8::1]:27005".
local function ExtractPlainIP(address)
    if not address or address == "" then
        return nil, "empty address"
    end

    if address == "loopback" then
        return nil, "loopback address"
    end

    -- Bracketed IPv6 with port: [2001:db8::1]:27005
    local ipv6 = address:match("^%[([^%]]+)%]:?%d*$")
    if ipv6 then
        return ipv6
    end

    -- IPv4 with optional port: 1.2.3.4:27005
    local ipv4 = address:match("^(%d+%.%d+%.%d+%.%d+):?%d*$")
    if ipv4 then
        return ipv4
    end

    -- Already a plain IP address. This also allows unbracketed IPv6.
    return address
end

local function PrintIpInfo(ip, lang)
    lang = lang or "en"

    local info, err = maxminddb.ip_info(ip, lang)
    if not info then
        print("[MaxMindDB] Lookup failed for", ip, err)
        return nil, err
    end

    print("========== MaxMindDB IP Info ==========")
    print("IP:", info.ip)

    print("-- Network --")
    print("ASN:", info.network.asn or "unknown")
    print("Range:", info.network.range or "unknown")
    print("Provider:", info.network.provider or "unknown")
    print("Organisation:", info.network.organisation or "unknown")
    print("Hostname:", info.network.hostname or "nil")

    print("-- Country --")
    print("Country code:", info.country.code or "unknown")
    print("Country name:", info.country.name or "unknown")
    print("Registered country code:", info.country.registered_code or "unknown")
    print("Registered country name:", info.country.registered_name or "unknown")
    print("Continent code:", info.country.continent_code or "unknown")
    print("Continent name:", info.country.continent_name or "unknown")

    return info
end

-- Manual test:
-- mmdb_test_ip 1.1.1.1 en
-- mmdb_test_ip 8.8.8.8 zh-CN
concommand.Add("mmdb_test_ip", function(ply, cmd, args)
    if IsValid(ply) and not ply:IsAdmin() then return end

    local ip = args[1] or "1.1.1.1"
    local lang = args[2] or "en"

    PrintIpInfo(ip, lang)
end)

-- Example: query players when they join.
hook.Add("PlayerInitialSpawn", "MaxMindDB_PrintPlayerIpInfo", function(ply)
    timer.Simple(2, function()
        if not IsValid(ply) then return end

        local plainIp, parseErr = ExtractPlainIP(ply:IPAddress())
        if not plainIp then
            print("[MaxMindDB] Skipping", ply:Nick(), parseErr)
            return
        end

        local info, err = maxminddb.ip_info(plainIp, "en")
        if not info then
            print("[MaxMindDB] Lookup failed for", ply:Nick(), plainIp, err)
            return
        end

        print(string.format(
            "[MaxMindDB] %s | %s | %s | %s | %s",
            ply:Nick(),
            plainIp,
            info.network.asn or "unknown ASN",
            info.network.organisation or "unknown organisation",
            info.country.code or "unknown country"
        ))

        -- Optional policy example. Do not blindly kick by country/ASN unless you understand the false-positive risk.
        -- if info.network.asn == "AS12345" then
        --     ply:Kick("This network is not allowed on this server.")
        -- end
    end)
end)
```

---

# API Reference

## `maxminddb.VERSION`

Module version string.

```lua
print(maxminddb.VERSION)
```

---

## `maxminddb.refresh()`

Reloads databases from disk.

```lua
local ok, err = maxminddb.refresh()
```

Returns:

```lua
true
```

or:

```lua
false, "error message"
```

---

## `maxminddb.ip_info(ipAddress, lang = "en")`

Combined ASN + Country lookup.

```lua
local info, err = maxminddb.ip_info("1.1.1.1", "en")
```

Returns:

```lua
{
    ip = "1.1.1.1",
    network = {
        asn = "AS13335",
        range = "1.1.1.0/24",
        hostname = nil,
        provider = "Cloudflare, Inc.",
        organisation = "Cloudflare, Inc."
    },
    country = {
        code = "AU",
        name = "Australia",
        registered_code = "AU",
        registered_name = "Australia",
        continent_code = "OC",
        continent_name = "Oceania"
    }
}
```

or:

```lua
nil, "error message"
```

### Notes

- `network.asn` is formatted as `AS<number>`, for example `AS13335`.
- `network.range` is the CIDR range returned by the ASN database.
- `network.provider` and `network.organisation` both use MaxMind's ASN organisation field when only GeoLite2 ASN is installed.
- `network.hostname` is always `nil` unless you add separate reverse DNS support.
- All fields may be `nil` if the database has no value for that IP.

Supported language values:

```text
de, en, es, fr, ja, pt-BR, ru, zh-CN
```

Unknown languages fall back to English and then to other available names.

---

## `maxminddb.country(ipAddress, lang = "en")`

Convenience country-name lookup.

```lua
local country, err = maxminddb.country("1.1.1.1", "en")

if country then
    print(country)
else
    print(err)
end
```

Example output:

```text
Australia
```

---

## `maxminddb.query(ipAddress, record)`

Advanced raw record lookup.

```lua
local data, err = maxminddb.query("1.1.1.1", maxminddb.records.Country)
```

Returns:

```lua
table
```

or:

```lua
nil, "error message"
```

Available records:

```lua
maxminddb.records.AnonymousIp
maxminddb.records.Asn
maxminddb.records.City
maxminddb.records.ConnectionType
maxminddb.records.Country
maxminddb.records.DensityIncome
maxminddb.records.Domain
maxminddb.records.Isp
```

Record database routing:

| Record | Database used |
|---|---|
| `Asn` | `GeoLite2-ASN.mmdb` |
| `Country` | `GeoLite2-Country.mmdb` |
| All other records | Optional default database: `garrysmod/maxminddb.mmdb` or `garrysmod/data/maxminddb.dat` |

Example raw ASN query:

```lua
local asn, err = maxminddb.query("1.1.1.1", maxminddb.records.Asn)

if not asn then
    error(err)
end

print(asn.autonomous_system_number)
print(asn.autonomous_system_organization)
```

Example raw country query:

```lua
local country, err = maxminddb.query("1.1.1.1", maxminddb.records.Country)

if not country then
    error(err)
end

PrintTable(country)
```

Example output shape:

```lua
continent = {
    code = "OC",
    geoname_id = 6255151,
    names = {
        de = "Ozeanien",
        en = "Oceania",
        es = "Oceanía",
        fr = "Océanie",
        ja = "オセアニア",
        ["pt-BR"] = "Oceania",
        ru = "Океания",
        ["zh-CN"] = "大洋洲"
    }
}

country = {
    geoname_id = 2077456,
    iso_code = "AU",
    names = {
        de = "Australien",
        en = "Australia",
        es = "Australia",
        fr = "Australie",
        ja = "オーストラリア",
        ["pt-BR"] = "Austrália",
        ru = "Австралия",
        ["zh-CN"] = "澳大利亚"
    }
}
```

---

# Rust -> Lua type conversions

Not every database has every field. Free GeoLite2 databases contain less data than paid GeoIP2 databases. Treat every returned field as possibly `nil`.

| Rust | Lua | Meaning |
|---|---|---|
| `Option<T>` | `T \| nil` | Optional value |
| `&str` / `String` | `string` | Text |
| `bool` | `boolean` | Boolean |
| `u8`, `u16`, `u32` | `number` | Positive integer |
| `f64` | `number` | Decimal number |
| `Vec<T>` | `{ T, ... }` | Sequential table |
| `Names` | `{ en = ..., de = ..., ... }` | Localized names table |

`maxminddb 0.28.x` uses a fixed `Names` struct internally, but this module exposes it to Lua as a language-code table for compatibility:

```lua
names = {
    de = "...",
    en = "...",
    es = "...",
    fr = "...",
    ja = "...",
    ["pt-BR"] = "...",
    ru = "...",
    ["zh-CN"] = "..."
}
```

---

# Common errors

## `You didn't install the required MaxMindDB database`

You are missing one of the required files:

```text
GeoLite2-ASN.mmdb
GeoLite2-Country.mmdb
```

Place them in `garrysmod/` or `garrysmod/data/` and run:

```lua
maxminddb.refresh()
```

## `This GeoIP record requires a default/full database`

You called `maxminddb.query()` with a record other than `Asn` or `Country`, but did not install the optional default database.

Install a compatible `.mmdb` at:

```text
garrysmod/maxminddb.mmdb
```

or:

```text
garrysmod/data/maxminddb.dat
```

## `Invalid IP address`

Pass a plain IP address, not a SteamID, hostname, URL, or `ip:port` string.

Correct:

```lua
maxminddb.ip_info("1.1.1.1")
```

Incorrect:

```lua
maxminddb.ip_info("1.1.1.1:27005")
```

Strip the port before calling the function.

---

# Limitations

## No hostname from GeoLite2 ASN/Country

`hostname` is always `nil` in `ip_info()` because GeoLite2 ASN and GeoLite2 Country do not include reverse DNS data.

## No VPN/proxy detection from ASN/Country alone

ASN and Country data can tell you who owns or announces an IP range, but it cannot reliably say whether the IP is a VPN, proxy, Tor node, or hosting provider.

For VPN/proxy/hosting detection, use one of these:

- MaxMind Anonymous IP database
- GeoIP2 Anonymous IP
- IPQualityScore
- proxycheck.io
- IPinfo privacy data
- another IP risk provider

## Do not blindly ban by country or ASN

Country and ASN blocking can cause false positives, especially for:

- mobile carriers
- universities
- shared networks
- cloud gaming users
- internet cafes
- corporate networks

Use logs, allowlists, and appeal handling if you enforce network-based restrictions.

---

# Developers

This module is built around the Rust `maxminddb` crate.

The upgraded version targets `maxminddb 0.28.x`, where lookup works like this internally:

```rust
let result = reader.lookup(ip)?;
let network = result.network()?;
let record = result.decode::<maxminddb::geoip2::Asn>()?;
```

The module keeps old Lua-facing APIs where possible:

```lua
maxminddb.refresh()
maxminddb.country(ip, lang)
maxminddb.query(ip, record)
```

and adds:

```lua
maxminddb.ip_info(ip, lang)
```

Recommended `Cargo.toml` dependency:

```toml
[dependencies]
maxminddb = "0.28"
thiserror = "2"
```

Optional performance features can be enabled depending on your build target:

```toml
maxminddb = { version = "0.28", features = ["mmap"] }
```

If you enable `mmap`, update the reader type and open method accordingly.

---

# License

Follow the original module license and MaxMind's database license terms. GeoLite2 databases require a MaxMind account and must be updated regularly for accurate results.
