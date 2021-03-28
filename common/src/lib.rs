use {
    semver::Version,
};

use macros::cargo_pkg_version;

pub const VERSION: Version = cargo_pkg_version!();
