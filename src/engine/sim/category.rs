//! Entity categories: sets of entity blueprints as fixed-width bitsets.
//!
//! The domain is every registered unit/projectile/prop blueprint, indexed in
//! registration order (deterministic: FAF registers via `sortedpairs`). Set ops
//! are word-wise and allocation-light; membership is a single bit test.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct CategorySet {
    words: Arc<[u64]>,
}

impl CategorySet {
    pub fn empty(domain_len: usize) -> Self {
        Self { words: vec![0u64; domain_len.div_ceil(64)].into() }
    }

    /// The set as its words (bit `i` of word `i / 64`: member `i`).
    pub fn words(&self) -> &[u64] {
        &self.words
    }

    /// A set from [`Self::words`].
    pub fn from_words(words: Vec<u64>) -> Self {
        Self { words: words.into() }
    }

    pub fn from_indices(domain_len: usize, indices: impl IntoIterator<Item = u32>) -> Self {
        let mut words = vec![0u64; domain_len.div_ceil(64)];
        for i in indices {
            words[i as usize / 64] |= 1 << (i % 64);
        }
        Self { words: words.into() }
    }

    fn zip(&self, other: &Self, f: impl Fn(u64, u64) -> u64) -> Self {
        let n = self.words.len().max(other.words.len());
        let get = |w: &[u64], i: usize| w.get(i).copied().unwrap_or(0);
        Self {
            words: (0..n).map(|i| f(get(&self.words, i), get(&other.words, i))).collect(),
        }
    }

    pub fn union(&self, other: &Self) -> Self {
        self.zip(other, |a, b| a | b)
    }

    pub fn intersection(&self, other: &Self) -> Self {
        self.zip(other, |a, b| a & b)
    }

    pub fn difference(&self, other: &Self) -> Self {
        self.zip(other, |a, b| a & !b)
    }

    pub fn contains(&self, index: u32) -> bool {
        self.words
            .get(index as usize / 64)
            .is_some_and(|w| w & (1 << (index % 64)) != 0)
    }

    pub fn is_empty(&self) -> bool {
        self.words.iter().all(|&w| w == 0)
    }

    /// Member indices in ascending (deterministic) order.
    pub fn iter(&self) -> impl Iterator<Item = u32> + '_ {
        self.words.iter().enumerate().flat_map(|(wi, &w)| {
            let mut bits = w;
            std::iter::from_fn(move || {
                if bits == 0 {
                    return None;
                }
                let b = bits.trailing_zeros();
                bits &= bits - 1;
                Some(wi as u32 * 64 + b)
            })
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityKind {
    Unit,
    Projectile,
    Prop,
}

/// The category domain plus the named categories built from blueprints.
#[derive(Default)]
pub struct CategoryDomain {
    /// Blueprint id and kind per domain index.
    pub entries: Vec<(String, EntityKind)>,
    /// Lowercase blueprint id -> domain index (lookups only, never iterated).
    #[allow(clippy::disallowed_types)]
    index_by_id: HashMap<String, u32>,
    /// Category name -> set, sorted for deterministic exposure to Lua.
    pub named: BTreeMap<String, CategorySet>,
}

impl CategoryDomain {
    /// `blueprints` must be in registration order; each item is
    /// `(id, kind, category names)`.
    pub fn build(
        blueprints: impl IntoIterator<Item = (String, EntityKind, Vec<String>)>,
    ) -> Self {
        let mut entries = Vec::new();
        let mut members: BTreeMap<String, Vec<u32>> = BTreeMap::new();
        for (id, kind, cats) in blueprints {
            let index = entries.len() as u32;
            for c in cats {
                members.entry(c).or_default().push(index);
            }
            let all = match kind {
                EntityKind::Unit => Some("ALLUNITS"),
                EntityKind::Projectile => Some("ALLPROJECTILES"),
                EntityKind::Prop => None,
            };
            if let Some(all) = all {
                members.entry(all.to_owned()).or_default().push(index);
            }
            entries.push((id, kind));
        }
        let n = entries.len();
        #[allow(clippy::disallowed_types)]
        let mut index_by_id = HashMap::with_capacity(n);
        for (i, (id, kind)) in entries.iter().enumerate() {
            index_by_id.insert(id.to_lowercase(), i as u32);
            // Unit blueprint ids are categories of their own (`categories.uel0001`).
            if *kind == EntityKind::Unit {
                members.entry(id.to_lowercase()).or_default().push(i as u32);
            }
        }
        let named = members
            .into_iter()
            .map(|(k, v)| (k, CategorySet::from_indices(n, v)))
            .collect();
        Self { entries, index_by_id, named }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// The blueprint id at `index`.
    pub fn id(&self, index: u32) -> &str {
        &self.entries[index as usize].0
    }

    pub fn index_of(&self, blueprint_id: &str) -> Option<u32> {
        self.index_by_id
            .get(blueprint_id)
            .or_else(|| self.index_by_id.get(&blueprint_id.to_lowercase()))
            .copied()
    }
}

impl CategoryDomain {
    /// Parses a category expression (`"LAND * (TECH1 + TECH2) - ENGINEER"`).
    /// Operators: `+` union, `*` intersection, `-` difference, evaluated left
    /// to right (no precedence) with parentheses, matching FAF's
    /// `ParseEntityCategoryProperly`. Juxtaposition is intersection, as in the
    /// native syntax used by blueprints (`"MOBILE EXPERIMENTAL"`); `,` is union
    /// (`"UNTARGETABLE, HOVER"`). Unknown
    /// names and empty expressions are the empty set.
    pub fn parse(&self, expr: &str) -> Result<CategorySet, String> {
        let tokens = tokenize(expr)?;
        if tokens.is_empty() {
            return Ok(CategorySet::empty(self.len()));
        }
        let mut pos = 0;
        let set = self.parse_seq(&tokens, &mut pos)?;
        if pos != tokens.len() {
            return Err(format!("unexpected '{}' in category expression", tokens[pos]));
        }
        Ok(set)
    }

    fn parse_seq(&self, t: &[String], pos: &mut usize) -> Result<CategorySet, String> {
        let mut acc = self.parse_atom(t, pos)?;
        while let Some(op) = t.get(*pos).map(String::as_str) {
            if op == ")" {
                break;
            }
            let op = match op {
                "+" | "-" | "*" => {
                    *pos += 1;
                    op
                }
                // `A B` (or `A (B)`) = intersection
                _ => "*",
            };
            let rhs = self.parse_atom(t, pos)?;
            acc = match op {
                "+" => acc.union(&rhs),
                "*" => acc.intersection(&rhs),
                "-" => acc.difference(&rhs),
                other => return Err(format!("expected operator, found '{other}'")),
            };
        }
        Ok(acc)
    }

    fn parse_atom(&self, t: &[String], pos: &mut usize) -> Result<CategorySet, String> {
        let tok = t.get(*pos).ok_or("unexpected end of category expression")?;
        *pos += 1;
        match tok.as_str() {
            "(" => {
                let inner = self.parse_seq(t, pos)?;
                if t.get(*pos).map(String::as_str) != Some(")") {
                    return Err("missing ')' in category expression".into());
                }
                *pos += 1;
                Ok(inner)
            }
            "+" | "-" | "*" | ")" => Err(format!("expected category name, found '{tok}'")),
            name => Ok(self
                .named
                .get(name)
                .or_else(|| self.named.get(&name.to_lowercase()))
                .cloned()
                .unwrap_or_else(|| CategorySet::empty(self.len()))),
        }
    }
}

fn tokenize(expr: &str) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    for c in expr.chars() {
        // Where a name is due (at the start, after `,` or `(`, or after an
        // operator), an operator is part of the name, as in moho's parser,
        // which knows only `,` (union) and spaces (intersection): LOUD's
        // `'ALLUNITS,-WALL'` is ALLUNITS and the unknown name `-WALL` (an
        // empty set), so ALLUNITS (MohoEngine.dll
        // `RRuleGameRulesImpl::ParseEntityCategory` 0x10118500 -> 0x10145040:
        // `STR_GetToken` on "," then " ", unknown names add nothing).
        let name_due = cur.is_empty() && out.last().is_none_or(|t| matches!(t.as_str(), "+" | "-" | "*" | "("));
        match c {
            // `,` is union in blueprint lists ("UNTARGETABLE, HOVER").
            ',' => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                out.push("+".into());
            }
            '+' | '-' | '*' if name_due => cur.push(c),
            '(' | ')' | '+' | '-' | '*' => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                out.push(c.to_string());
            }
            c if c.is_whitespace() => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c if c.is_alphanumeric() || c == '_' => cur.push(c),
            other => return Err(format!("invalid character '{other}' in category expression")),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn domain() -> CategoryDomain {
        CategoryDomain::build([
            ("uel0001".into(), EntityKind::Unit, vec!["LAND".into(), "COMMAND".into()]),
            ("uel0105".into(), EntityKind::Unit, vec!["LAND".into(), "ENGINEER".into()]),
            ("/projectiles/a/a_proj.bp".into(), EntityKind::Projectile, vec!["MISSILE".into()]),
        ])
    }

    #[test]
    fn set_algebra_and_synthesized_categories() {
        let d = domain();
        let land = &d.named["LAND"];
        let eng = &d.named["ENGINEER"];
        let cmd = land.difference(eng);
        assert_eq!(cmd.iter().collect::<Vec<_>>(), vec![0]);
        assert_eq!(land.intersection(eng).iter().collect::<Vec<_>>(), vec![1]);
        assert_eq!(d.named["ALLUNITS"].iter().count(), 2);
        assert_eq!(d.named["ALLPROJECTILES"].iter().collect::<Vec<_>>(), vec![2]);
        assert!(d.named["uel0105"].contains(1));
        assert_eq!(d.index_of("UEL0105"), Some(1));
        assert!(CategorySet::empty(3).is_empty());
    }

    #[test]
    fn parses_expressions_left_to_right() {
        let d = domain();
        let ids = |e: &str| d.parse(e).unwrap().iter().collect::<Vec<_>>();
        assert_eq!(ids("LAND - ENGINEER"), vec![0]);
        assert_eq!(ids("LAND * (ENGINEER + COMMAND)"), vec![0, 1]);
        // left to right: (MISSILE + LAND) - COMMAND
        assert_eq!(ids("MISSILE + LAND - COMMAND"), vec![1, 2]);
        assert_eq!(ids("uel0105"), vec![1]);
        assert!(d.parse("NOPE").unwrap().is_empty());
        assert_eq!(ids("LAND ENGINEER"), vec![1]);
        assert!(d.parse("").unwrap().is_empty());
        assert_eq!(ids("COMMAND, ENGINEER"), vec![0, 1]);
        assert!(d.parse("LAND +").is_err());
        // An operator where a name is due is part of an (unknown) name, as
        // in moho: LOUD's `'ALLUNITS,-WALL'`.
        assert_eq!(ids("LAND,-ENGINEER"), vec![0, 1]);
        assert_eq!(ids("COMMAND, -ENGINEER"), vec![0]);
        assert!(d.parse("-LAND").unwrap().is_empty());
    }
}
