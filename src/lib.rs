#[macro_use] extern crate gmod;
#[macro_use] extern crate thiserror;

use std::{borrow::Cow, cell::RefCell, net::IpAddr, path::PathBuf, str::FromStr};
use maxminddb::MaxMindDbError;

mod serialize;
use serialize::PushToLua;

type DBReader = maxminddb::Reader<Vec<u8>>;

struct Databases {
	/// Optional legacy/full DB, used by query() for record types other than Asn/Country.
	/// For example: City, Isp, AnonymousIp, ConnectionType, etc.
	default: Option<DBReader>,
	asn: DBReader,
	country: DBReader,
}

enum MaybeBorrowed<'a, T> {
	Borrowed(&'a T),
	Owned(T),
}
impl<T> AsRef<T> for MaybeBorrowed<'_, T> {
	#[inline]
	fn as_ref(&self) -> &T {
		match self {
			MaybeBorrowed::Borrowed(borrowed) => borrowed,
			MaybeBorrowed::Owned(owned) => owned,
		}
	}
}

#[derive(Error, Debug)]
pub enum DBError {
	#[error("{0}")]
	Internal(#[from] MaxMindDbError),

	#[error("You didn't install the required MaxMindDB database: {0}")]
	NotInstalled(&'static str),

	#[error("This GeoIP record requires a default/full database, but garrysmod/maxminddb.mmdb or garrysmod/data/maxminddb.dat was not found")]
	DefaultNotInstalled,
}

fn open_first_existing(paths: &[&str]) -> Result<Option<DBReader>, DBError> {
	for path in paths {
		if PathBuf::from(path).exists() {
			return maxminddb::Reader::open_readfile(*path)
				.map(Some)
				.map_err(Into::into);
		}
	}

	Ok(None)
}

fn open_required(name: &'static str, paths: &[&str]) -> Result<DBReader, DBError> {
	open_first_existing(paths)?.ok_or(DBError::NotInstalled(name))
}

fn init_dbs() -> Result<Databases, DBError> {
	Ok(Databases {
		// Keep the old single-db locations as an optional fallback for query().
		default: open_first_existing(&[
			"garrysmod/maxminddb.mmdb",
			"garrysmod/data/maxminddb.dat",
			"garrysmod/data/maxminddb.mmdb",
		])?,

		asn: open_required(
			"maxminddb_asn.mmdb; expected garrysmod/maxminddb_asn.mmdb or garrysmod/data/maxminddb_asn.dat",
			&[
				"garrysmod/maxminddb_asn.mmdb",
				"garrysmod/data/maxminddb_asn.dat",
				"garrysmod/data/maxminddb_asn.mmdb",
			],
		)?,

		country: open_required(
			"GeoLite2-Country.mmdb; expected garrysmod/maxminddb_country.mmdb or garrysmod/data/maxminddb_country.dat",
			&[
				"garrysmod/maxminddb_country.mmdb",
				"garrysmod/data/maxminddb_country.dat",
				"garrysmod/data/maxminddb_country.mmdb",
			],
		)?,
	})
}

thread_local! {
	static DBS: RefCell<Result<Databases, DBError>> = RefCell::new(init_dbs());
}

fn name_for_lang<'a>(names: &maxminddb::geoip2::Names<'a>, lang: &str) -> Option<&'a str> {
	let preferred = match lang {
		"de" => names.german,
		"en" | "en-US" | "en_US" => names.english,
		"es" => names.spanish,
		"fr" => names.french,
		"ja" => names.japanese,
		"pt-BR" | "pt_BR" => names.brazilian_portuguese,
		"ru" => names.russian,
		"zh-CN" | "zh_CN" | "zh" => names.simplified_chinese,
		_ => None,
	};

	preferred
		.or(names.english)
		.or(names.simplified_chinese)
		.or(names.japanese)
		.or(names.spanish)
		.or(names.french)
		.or(names.german)
		.or(names.russian)
		.or(names.brazilian_portuguese)
}

unsafe fn set_nil_field(lua: gmod::lua::State, key: *const std::ffi::c_char) {
	lua.push_nil();
	lua.set_field(-2, key);
}

unsafe fn set_string_field(lua: gmod::lua::State, key: *const std::ffi::c_char, value: &str) {
	lua.push_string(value);
	lua.set_field(-2, key);
}

unsafe fn set_optional_string_field(lua: gmod::lua::State, key: *const std::ffi::c_char, value: Option<&str>) {
	if let Some(value) = value {
		set_string_field(lua, key, value);
	} else {
		set_nil_field(lua, key);
	}
}

#[lua_function]
unsafe fn refresh(lua: gmod::lua::State) -> i32 {
	DBS.with(|dbs| match init_dbs() {
		Ok(refreshed) => {
			lua.push_boolean(true);

			*dbs.borrow_mut() = Ok(refreshed);

			1
		}
		Err(refreshed) => {
			lua.push_boolean(false);
			lua.push_string(&refreshed.to_string());

			if let Err(ref mut err) = *dbs.borrow_mut() {
				*err = refreshed;
			}

			2
		}
	})
}

#[lua_function]
unsafe fn query(lua: gmod::lua::State) -> i32 {
	let ip_addr = match IpAddr::from_str(lua.check_string(1).as_ref()) {
		Ok(ip_addr) => ip_addr,
		Err(err) => lua.error(&format!("Invalid IP address: {}", err)),
	};

	let record = match GeoIPRecord::try_from(lua.check_integer(2)) {
		Ok(record) => record,
		Err(_) => lua.error("Unknown or invalid GeoIP record type"),
	};

	DBS.with(|dbs| {
		match dbs
			.borrow()
			.as_ref()
			.map_err(MaybeBorrowed::Borrowed)
			.and_then(|dbs| {
				record
					.lookup(lua, dbs, ip_addr)
					.map_err(MaybeBorrowed::Owned)
			}) {
			Ok(_) => 1,
			Err(err) => {
				lua.push_nil();
				lua.push_string(&err.as_ref().to_string());
				2
			}
		}
	})
}

#[lua_function]
unsafe fn country(lua: gmod::lua::State) -> i32 {
	let ip_addr = match IpAddr::from_str(lua.check_string(1).as_ref()) {
		Ok(ip_addr) => ip_addr,
		Err(err) => lua.error(&format!("Invalid IP address: {}", err)),
	};

	let lang = lua.get_string(2).unwrap_or(Cow::Borrowed("en"));

	DBS.with(|dbs| {
		match dbs
			.borrow()
			.as_ref()
			.map_err(MaybeBorrowed::Borrowed)
			.and_then(|dbs| {
				let result = dbs.country.lookup(ip_addr)
					.map_err(Into::into)
					.map_err(MaybeBorrowed::Owned)?;

				let country = result.decode::<maxminddb::geoip2::Country>()
					.map_err(Into::into)
					.map_err(MaybeBorrowed::Owned)?;

				Ok(country)
			}) {
			Ok(Some(country)) => {
				if let Some(country_name) = name_for_lang(&country.country.names, lang.as_ref()) {
					lua.push_string(country_name);
				} else {
					lua.push_nil();
				}
				1
			}
			Ok(None) => {
				lua.push_nil();
				1
			}
			Err(err) => {
				lua.push_nil();
				lua.push_string(&err.as_ref().to_string());
				2
			}
		}
	})
}

#[lua_function]
unsafe fn ip_info(lua: gmod::lua::State) -> i32 {
	let ip_addr = match IpAddr::from_str(lua.check_string(1).as_ref()) {
		Ok(ip_addr) => ip_addr,
		Err(err) => lua.error(&format!("Invalid IP address: {}", err)),
	};

	let lang = lua.get_string(2).unwrap_or(Cow::Borrowed("en"));

	DBS.with(|dbs| {
		let dbs_ref = dbs.borrow();
		let dbs = match dbs_ref.as_ref() {
			Ok(dbs) => dbs,
			Err(err) => {
				lua.push_nil();
				lua.push_string(&err.to_string());
				return 2;
			}
		};

		lua.new_table();

		// ip
		set_string_field(lua, lua_string!("ip"), &ip_addr.to_string());

		// network
		lua.new_table();

		let asn_lookup = match dbs.asn.lookup(ip_addr) {
			Ok(result) => result,
			Err(err) => {
				lua.push_nil();
				lua.push_string(&err.to_string());
				return 2;
			}
		};

		match asn_lookup.network() {
			Ok(network) => set_string_field(lua, lua_string!("range"), &network.to_string()),
			Err(_) => set_nil_field(lua, lua_string!("range")),
		}

		match asn_lookup.decode::<maxminddb::geoip2::Asn>() {
			Ok(Some(asn)) => {
				if let Some(asn_number) = asn.autonomous_system_number {
					set_string_field(lua, lua_string!("asn"), &format!("AS{}", asn_number));
				} else {
					set_nil_field(lua, lua_string!("asn"));
				}

				set_optional_string_field(
					lua,
					lua_string!("organisation"),
					asn.autonomous_system_organization,
				);

				set_optional_string_field(
					lua,
					lua_string!("provider"),
					asn.autonomous_system_organization,
				);
			}
			Ok(None) => {
				set_nil_field(lua, lua_string!("asn"));
				set_nil_field(lua, lua_string!("organisation"));
				set_nil_field(lua, lua_string!("provider"));
			}
			Err(err) => {
				lua.push_nil();
				lua.push_string(&err.to_string());
				return 2;
			}
		}

		// GeoLite2 ASN/Country does not contain reverse DNS hostname.
		set_nil_field(lua, lua_string!("hostname"));

		lua.set_field(-2, lua_string!("network"));

		// country
		lua.new_table();

		let country_lookup = match dbs.country.lookup(ip_addr) {
			Ok(result) => result,
			Err(err) => {
				lua.push_nil();
				lua.push_string(&err.to_string());
				return 2;
			}
		};

		match country_lookup.decode::<maxminddb::geoip2::Country>() {
			Ok(Some(country)) => {
				set_optional_string_field(lua, lua_string!("code"), country.country.iso_code);
				set_optional_string_field(lua, lua_string!("name"), name_for_lang(&country.country.names, lang.as_ref()));

				set_optional_string_field(lua, lua_string!("registered_code"), country.registered_country.iso_code);
				set_optional_string_field(lua, lua_string!("registered_name"), name_for_lang(&country.registered_country.names, lang.as_ref()));

				set_optional_string_field(lua, lua_string!("continent_code"), country.continent.code);
				set_optional_string_field(lua, lua_string!("continent_name"), name_for_lang(&country.continent.names, lang.as_ref()));
			}
			Ok(None) => {
				set_nil_field(lua, lua_string!("code"));
				set_nil_field(lua, lua_string!("name"));
				set_nil_field(lua, lua_string!("registered_code"));
				set_nil_field(lua, lua_string!("registered_name"));
				set_nil_field(lua, lua_string!("continent_code"));
				set_nil_field(lua, lua_string!("continent_name"));
			}
			Err(err) => {
				lua.push_nil();
				lua.push_string(&err.to_string());
				return 2;
			}
		}

		lua.set_field(-2, lua_string!("country"));

		1
	})
}

macro_rules! geoip_records {
	{$first_record:ident, $($record:ident),*} => {
		#[repr(isize)]
		pub enum GeoIPRecord {
			$first_record = 0,
			$($record,)*
		}
		impl TryFrom<isize> for GeoIPRecord {
			type Error = isize;

			fn try_from(value: isize) -> Result<Self, Self::Error> {
				#[allow(non_upper_case_globals)] const $first_record: isize = GeoIPRecord::$first_record as isize;
				$(#[allow(non_upper_case_globals)] const $record: isize = GeoIPRecord::$record as isize;)*

				#[allow(non_upper_case_globals)]
				match value {
					$first_record => Ok(GeoIPRecord::$first_record),
					$($record => Ok(GeoIPRecord::$record),)*
					_ => Err(value)
				}
			}
		}
		impl GeoIPRecord {
			fn reader<'a>(&self, dbs: &'a Databases) -> Result<&'a DBReader, DBError> {
				match self {
					GeoIPRecord::Asn => Ok(&dbs.asn),
					GeoIPRecord::Country => Ok(&dbs.country),
					_ => dbs.default.as_ref().ok_or(DBError::DefaultNotInstalled),
				}
			}

			fn lookup(self, lua: gmod::lua::State, dbs: &Databases, ip_addr: IpAddr) -> Result<(), DBError> {
				let db = self.reader(dbs)?;
				unsafe {
					match self {
						GeoIPRecord::$first_record => {
							let result = db.lookup(ip_addr)?;
							if let Some(record) = result.decode::<maxminddb::geoip2::$first_record>()? {
								record.push_to_lua(lua);
							} else {
								lua.push_nil();
							}
						},
						$(GeoIPRecord::$record => {
							let result = db.lookup(ip_addr)?;
							if let Some(record) = result.decode::<maxminddb::geoip2::$record>()? {
								record.push_to_lua(lua);
							} else {
								lua.push_nil();
							}
						},)*
					}
				}
				Ok(())
			}
		}

		#[gmod13_open]
		unsafe fn gmod13_open(lua: gmod::lua::State) -> i32 {
			lua_stack_guard!(lua => {
				lua.new_table();

				lua.push_string(env!("CARGO_PKG_VERSION"));
				lua.set_field(-2, lua_string!("VERSION"));

				lua.push_function(refresh);
				lua.set_field(-2, lua_string!("refresh"));

				lua.push_function(country);
				lua.set_field(-2, lua_string!("country"));

				lua.push_function(query);
				lua.set_field(-2, lua_string!("query"));

				lua.push_function(ip_info);
				lua.set_field(-2, lua_string!("ip_info"));

				lua.new_table();

				lua.push_integer(GeoIPRecord::$first_record as isize);
				lua.set_field(-2, concat!(stringify!($first_record), "\0").as_ptr() as *const _);

				$(
					lua.push_integer(GeoIPRecord::$record as isize);
					lua.set_field(-2, concat!(stringify!($record), "\0").as_ptr() as *const _);
				)*

				lua.set_field(-2, lua_string!("records"));

				lua.set_global(lua_string!("maxminddb"));
			});
			0
		}
	};
}
geoip_records! {
	AnonymousIp,
	Asn,
	City,
	ConnectionType,
	Country,
	DensityIncome,
	Domain,
	Isp
}
