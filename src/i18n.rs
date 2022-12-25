// i18n.rs
//
// Copyright 2020 Christopher Davis <christopherdavis@gnome.org>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.
//
// SPDX-License-Identifier: GPL-3.0-or-later

use gettextrs::{gettext, ngettext, npgettext, pgettext};
use regex::{Captures, Regex};

#[allow(dead_code)]
fn freplace(input: String, args: &[&str]) -> String {
    let mut parts = input.split("{}");
    let mut output = parts.next().unwrap_or_default().to_string();
    for (p, a) in parts.zip(args.iter()) {
        output += &(a.to_string() + p);
    }
    output
}

#[allow(dead_code)]
fn kreplace(input: String, kwargs: &[(&str, &str)]) -> String {
    let mut s = input;
    for (k, v) in kwargs {
        if let Ok(re) = Regex::new(&format!("\\{{{}\\}}", k)) {
            s = re
                .replace_all(&s, |_: &Captures<'_>| v.to_string())
                .to_string();
        }
    }

    s
}

// Simple translations functions

#[allow(dead_code)]
pub fn i18n(format: &str) -> String {
    gettext(format)
}

#[allow(dead_code)]
pub fn i18n_f(format: &str, args: &[&str]) -> String {
    let s = gettext(format);
    freplace(s, args)
}

#[allow(dead_code)]
pub fn i18n_k(format: &str, kwargs: &[(&str, &str)]) -> String {
    let s = gettext(format);
    kreplace(s, kwargs)
}

// Singular and plural translations functions

#[allow(dead_code)]
pub fn ni18n(single: &str, multiple: &str, number: u32) -> String {
    ngettext(single, multiple, number)
}

#[allow(dead_code)]
pub fn ni18n_f(single: &str, multiple: &str, number: u32, args: &[&str]) -> String {
    let s = ngettext(single, multiple, number);
    freplace(s, args)
}

#[allow(dead_code)]
pub fn ni18n_k(single: &str, multiple: &str, number: u32, kwargs: &[(&str, &str)]) -> String {
    let s = ngettext(single, multiple, number);
    kreplace(s, kwargs)
}

// Translations with context functions

#[allow(dead_code)]
pub fn pi18n(ctx: &str, format: &str) -> String {
    pgettext(ctx, format)
}

#[allow(dead_code)]
pub fn pi18n_f(ctx: &str, format: &str, args: &[&str]) -> String {
    let s = pgettext(ctx, format);
    freplace(s, args)
}

#[allow(dead_code)]
pub fn pi18n_k(ctx: &str, format: &str, kwargs: &[(&str, &str)]) -> String {
    let s = pgettext(ctx, format);
    kreplace(s, kwargs)
}

// Singular and plural with context

#[allow(dead_code)]
pub fn pni18n(ctx: &str, single: &str, multiple: &str, number: u32) -> String {
    npgettext(ctx, single, multiple, number)
}

#[allow(dead_code)]
pub fn pni18n_f(ctx: &str, single: &str, multiple: &str, number: u32, args: &[&str]) -> String {
    let s = npgettext(ctx, single, multiple, number);
    freplace(s, args)
}

#[allow(dead_code)]
pub fn pni18n_k(
    ctx: &str,
    single: &str,
    multiple: &str,
    number: u32,
    kwargs: &[(&str, &str)],
) -> String {
    let s = npgettext(ctx, single, multiple, number);
    kreplace(s, kwargs)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_i18n() {
        let out = i18n("translate1");
        assert_eq!(out, "translate1");

        let out = ni18n("translate1", "translate multi", 1);
        assert_eq!(out, "translate1");
        let out = ni18n("translate1", "translate multi", 2);
        assert_eq!(out, "translate multi");
    }

    #[test]
    fn test_i18n_f() {
        let out = i18n_f("{} param", &["one"]);
        assert_eq!(out, "one param");

        let out = i18n_f("middle {} param", &["one"]);
        assert_eq!(out, "middle one param");

        let out = i18n_f("end {}", &["one"]);
        assert_eq!(out, "end one");

        let out = i18n_f("multiple {} and {}", &["one", "two"]);
        assert_eq!(out, "multiple one and two");

        let out = ni18n_f("singular {} and {}", "plural {} and {}", 2, &["one", "two"]);
        assert_eq!(out, "plural one and two");
        let out = ni18n_f("singular {} and {}", "plural {} and {}", 1, &["one", "two"]);
        assert_eq!(out, "singular one and two");
    }

    #[test]
    fn test_i18n_k() {
        let out = i18n_k("{one} param", &[("one", "one")]);
        assert_eq!(out, "one param");

        let out = i18n_k("middle {one} param", &[("one", "one")]);
        assert_eq!(out, "middle one param");

        let out = i18n_k("end {one}", &[("one", "one")]);
        assert_eq!(out, "end one");

        let out = i18n_k("multiple {one} and {two}", &[("one", "1"), ("two", "two")]);
        assert_eq!(out, "multiple 1 and two");

        let out = i18n_k("multiple {two} and {one}", &[("one", "1"), ("two", "two")]);
        assert_eq!(out, "multiple two and 1");

        let out = i18n_k("multiple {one} and {one}", &[("one", "1"), ("two", "two")]);
        assert_eq!(out, "multiple 1 and 1");

        let out = ni18n_k(
            "singular {one} and {two}",
            "plural {one} and {two}",
            1,
            &[("one", "1"), ("two", "two")],
        );
        assert_eq!(out, "singular 1 and two");
        let out = ni18n_k(
            "singular {one} and {two}",
            "plural {one} and {two}",
            2,
            &[("one", "1"), ("two", "two")],
        );
        assert_eq!(out, "plural 1 and two");
    }

    #[test]
    fn test_pi18n() {
        let out = pi18n("This is the context", "translate1");
        assert_eq!(out, "translate1");

        let out = pni18n("context", "translate1", "translate multi", 1);
        assert_eq!(out, "translate1");
        let out = pni18n("The context string", "translate1", "translate multi", 2);
        assert_eq!(out, "translate multi");

        let out = pi18n_f("Context for translation", "{} param", &["one"]);
        assert_eq!(out, "one param");

        let out = pi18n_k("context", "{one} param", &[("one", "one")]);
        assert_eq!(out, "one param");
    }
}
