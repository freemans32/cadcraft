//! What the quantities of a takeoff cost, and the total of several levels. The prices are the user's, by line (a key made of the group and
//! the label, "Duct|Ø200", "Pipe|chilledWater|DN50", "Elbow 90°|Ø200"); a line without a price counts for nothing and is reported as unpriced.
//! The ducts insulated are chosen by direction of air, the pipes by the insulation each carries.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::AirDirection;
use super::takeoff::{CountQuantity, DuctQuantity, LevelTotals, Takeoff, TerminalQuantity, Totals, UnitQuantity};

/// The user's prices and choices for a takeoff.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TakeoffSettings {
    /// The unit price of each line, by key (euros per metre, per square metre or each; see [`CostLine::unit`]).
    #[serde(default)]
    pub prices: BTreeMap<String, f64>,
    /// The directions of air whose ducts are insulated (the surface is counted and priced under "Duct insulation").
    #[serde(default)]
    pub insulated_air: Vec<AirDirection>,
}

/// One line of the cost.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CostLine {
    pub group: String,
    pub label: String,
    /// "m", "m²" or "each".
    pub unit: String,
    pub quantity: f64,
    /// The key of the price in [`TakeoffSettings::prices`].
    pub key: String,
    pub unit_price: Option<f64>,
    /// The quantity times the price, when there is a price.
    pub total: Option<f64>,
}

/// The lines of a takeoff with their prices, and what they come to.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Costing {
    pub lines: Vec<CostLine>,
    pub total: f64,
    /// The lines that have no price.
    pub unpriced: usize,
}

/// The most a price may be, euros: a typo, not a price, above.
const MAX_PRICE: f64 = 1.0e9;

/// The price a user typed, if it is a usable one.
pub fn usable_price(v: f64) -> Option<f64> {
    (v.is_finite() && (0.0..=MAX_PRICE).contains(&v)).then_some(v)
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

impl Takeoff {
    /// The lines of the takeoff that have a price (or could have one), in the order of the tables, each with the price of `settings`.
    pub fn costing(&self, settings: &TakeoffSettings) -> Costing {
        let mut lines: Vec<CostLine> = Vec::new();
        let mut push = |group: &str, label: String, unit: &str, quantity: f64, key: String| {
            let unit_price = settings.prices.get(&key).copied().and_then(usable_price);
            let total = unit_price.map(|p| round2(p * quantity));
            lines.push(CostLine { group: group.into(), label, unit: unit.into(), quantity: round2(quantity), key, unit_price, total });
        };
        // Ducts: rigid and flexible by section, whatever the direction of air.
        let mut ducts: BTreeMap<String, f64> = BTreeMap::new();
        let mut insulated = 0.0;
        for d in &self.ducts {
            let name = if d.flexible { format!("{} (flexible)", d.section) } else { d.section.clone() };
            *ducts.entry(name).or_default() += d.length_m;
            if settings.insulated_air.contains(&d.air) && !d.flexible {
                insulated += d.area_m2;
            }
        }
        for (name, m) in ducts {
            push("Duct", name.clone(), "m", m, format!("Duct|{name}"));
        }
        if insulated > 0.0 {
            push("Duct insulation", String::new(), "m²", insulated, "Duct insulation".into());
        }
        // The insulation some runs carry by themselves, by side and thickness.
        for q in &self.duct_insulation {
            let label = format!("{} {:.0} mm", q.side, q.thickness_mm);
            push("Duct insulation", label.clone(), "m²", q.area_m2, format!("Duct insulation|{label}"));
        }
        // Pipes by service and size, whatever the network and the leg.
        let mut pipes: BTreeMap<(String, String), f64> = BTreeMap::new();
        for q in &self.pipes {
            *pipes.entry((q.service.name().to_string(), q.size.clone())).or_default() += q.length_m;
        }
        for ((service, size), m) in pipes {
            push("Pipe", format!("{service} {size}"), "m", m, format!("Pipe|{service}|{size}"));
        }
        let pipe_insulation: f64 = self.pipes.iter().map(|q| q.insulation_m2).sum();
        if pipe_insulation > 0.0 {
            push("Pipe insulation", String::new(), "m²", pipe_insulation, "Pipe insulation".into());
        }
        // Things counted.
        for (group, list) in
            [("Fitting", &self.fittings), ("Accessory", &self.accessories), ("Pipework part", &self.parts), ("Equipment", &self.equipment)]
        {
            for c in list {
                let label = if c.size.is_empty() { c.name.clone() } else { format!("{} {}", c.name, c.size) };
                push(group, label, "each", c.count as f64, format!("{group}|{}|{}", c.name, c.size));
            }
        }
        // A model is priced once, whatever its flows.
        let mut models: Vec<(&str, usize)> = Vec::new();
        for q in &self.terminals {
            match models.iter_mut().find(|m| m.0 == q.model) {
                Some(m) => m.1 += q.count,
                None => models.push((q.model.as_str(), q.count)),
            }
        }
        for (model, count) in models {
            push("Terminal", model.to_string(), "each", count as f64, format!("Terminal|{model}"));
        }
        for u in &self.units {
            push("Unit", u.name.clone(), "each", 1.0, format!("Unit|{}", u.name));
        }
        let total = round2(lines.iter().filter_map(|l| l.total).sum());
        let unpriced = lines.iter().filter(|l| l.unit_price.is_none()).count();
        Costing { lines, total, unpriced }
    }

    /// The quantities of several levels added together under the name `name`: ducts and pipes with the same size add up (an altitude is
    /// not kept), things counted by name and size, terminals by model; spaces and risers stay with their level. Each level's totals are kept
    /// in [`Takeoff::levels`].
    pub fn building(name: &str, levels: &[Takeoff]) -> Takeoff {
        let mut b = Takeoff { level: name.to_string(), ..Takeoff::default() };
        for l in levels {
            for d in &l.ducts {
                match b.ducts.iter_mut().find(|x| x.air == d.air && x.flexible == d.flexible && x.section == d.section) {
                    Some(x) => {
                        x.count += d.count;
                        x.length_m = round2(x.length_m + d.length_m);
                        x.area_m2 = round2(x.area_m2 + d.area_m2);
                        x.weight_kg = (x.weight_kg * 10.0 + d.weight_kg * 10.0).round() / 10.0;
                    }
                    None => b.ducts.push(DuctQuantity { altitude_m: None, ..d.clone() }),
                }
            }
            for q in &l.duct_insulation {
                match b.duct_insulation.iter_mut().find(|x| x.side == q.side && (x.thickness_mm - q.thickness_mm).abs() < 0.5) {
                    Some(x) => {
                        x.length_m = round2(x.length_m + q.length_m);
                        x.area_m2 = round2(x.area_m2 + q.area_m2);
                        x.volume_m3 = round2(x.volume_m3 + q.volume_m3);
                    }
                    None => b.duct_insulation.push(q.clone()),
                }
            }
            for p in &l.pipes {
                match b.pipes.iter_mut().find(|x| {
                    x.service == p.service
                        && x.leg == p.leg
                        && x.size == p.size
                        && x.insulation_mm == p.insulation_mm
                        && super::same_network(&x.network, &p.network)
                }) {
                    Some(x) => {
                        x.count += p.count;
                        x.length_m = round2(x.length_m + p.length_m);
                        x.insulation_m2 = round2(x.insulation_m2 + p.insulation_m2);
                    }
                    None => b.pipes.push(p.clone()),
                }
            }
            for q in &l.subnetworks {
                match b
                    .subnetworks
                    .iter_mut()
                    .find(|x| x.kind == q.kind && super::same_network(&x.network, &q.network) && x.name.eq_ignore_ascii_case(&q.name))
                {
                    Some(x) => {
                        x.count += q.count;
                        x.length_m = round2(x.length_m + q.length_m);
                        for s in &q.sizes {
                            if !x.sizes.contains(s) {
                                x.sizes.push(s.clone());
                            }
                        }
                    }
                    None => b.subnetworks.push(q.clone()),
                }
            }
            for (dst, src) in
                [(&mut b.fittings, &l.fittings), (&mut b.accessories, &l.accessories), (&mut b.parts, &l.parts), (&mut b.equipment, &l.equipment)]
            {
                merge_counts(dst, src);
            }
            for q in &l.terminals {
                match b.terminals.iter_mut().find(|x| x.model == q.model && x.air == q.air && (x.flow_m3h - q.flow_m3h).abs() < 0.5) {
                    Some(x) => x.count += q.count,
                    None => b.terminals.push(TerminalQuantity { ..q.clone() }),
                }
            }
            b.units.extend(l.units.iter().map(|u| UnitQuantity { name: format!("{} ({})", u.name, l.level), ..u.clone() }));
            b.risers.extend(l.risers.iter().cloned());
            b.levels.push(LevelTotals { level: l.level.clone(), totals: l.totals.clone() });
        }
        b.totals = sum_totals(levels.iter().map(|l| &l.totals));
        b.totals.duct_weight_kg = (b.ducts.iter().map(|d| d.weight_kg).sum::<f64>() * 10.0).round() / 10.0;
        b
    }
}

fn merge_counts(dst: &mut Vec<CountQuantity>, src: &[CountQuantity]) {
    for c in src {
        match dst.iter_mut().find(|x| x.name == c.name && x.size == c.size) {
            Some(x) => {
                x.count += c.count;
                x.power = match (x.power, c.power) {
                    (Some(a), Some(b)) => Some(a + b),
                    (a, b) => a.or(b),
                };
            }
            None => dst.push(c.clone()),
        }
    }
}

fn sum_totals<'a>(all: impl Iterator<Item = &'a Totals>) -> Totals {
    let mut t = Totals::default();
    for x in all {
        t.pipe_count += x.pipe_count;
        t.pipe_length_m = round2(t.pipe_length_m + x.pipe_length_m);
        t.pipe_insulation_m2 = round2(t.pipe_insulation_m2 + x.pipe_insulation_m2);
        t.pipe_insulation_m3 = (t.pipe_insulation_m3 * 1000.0 + x.pipe_insulation_m3 * 1000.0).round() / 1000.0;
        t.equipment_weight_kg = (t.equipment_weight_kg * 10.0 + x.equipment_weight_kg * 10.0).round() / 10.0;
        t.total_weight_kg = (t.total_weight_kg * 10.0 + x.total_weight_kg * 10.0).round() / 10.0;
        t.duct_count += x.duct_count;
        t.duct_length_m = round2(t.duct_length_m + x.duct_length_m);
        t.duct_area_m2 = round2(t.duct_area_m2 + x.duct_area_m2);
        t.duct_weight_kg = (t.duct_weight_kg * 10.0 + x.duct_weight_kg * 10.0).round() / 10.0;
        t.terminals += x.terminals;
        t.equipment += x.equipment;
        t.riser_length_m = round2(t.riser_length_m + x.riser_length_m);
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level(name: &str, length: f64, elbows: usize) -> Takeoff {
        let mut t = Takeoff { level: name.into(), ..Takeoff::default() };
        t.ducts.push(DuctQuantity {
            section: "Ø200".into(),
            count: 1,
            length_m: length,
            area_m2: length * 0.628,
            weight_kg: length * 3.0,
            ..DuctQuantity::default()
        });
        t.fittings.push(CountQuantity { name: "Elbow 90°".into(), size: "Ø200".into(), count: elbows, power: None, weight_kg: None });
        t.totals = Totals { duct_count: 1, duct_length_m: length, duct_area_m2: length * 0.628, duct_weight_kg: length * 3.0, ..Totals::default() };
        t
    }

    #[test]
    fn a_priced_line_costs_its_quantity_and_an_unpriced_one_is_reported() {
        let t = level("RDC", 10.0, 3);
        let mut s = TakeoffSettings::default();
        s.prices.insert("Duct|Ø200".into(), 20.0);
        s.prices.insert("Elbow 90°|Ø200".into(), f64::NAN);
        let c = t.costing(&s);
        assert_eq!(c.total, 200.0);
        assert_eq!(c.unpriced, 1, "a hostile price counts as none");
    }

    #[test]
    fn levels_add_up_and_keep_their_totals() {
        let b = Takeoff::building("Bâtiment", &[level("RDC", 10.0, 3), level("R+1", 5.0, 2)]);
        assert_eq!((b.ducts.len(), b.ducts[0].length_m, b.fittings[0].count), (1, 15.0, 5));
        assert_eq!((b.totals.duct_length_m, b.totals.duct_weight_kg, b.levels.len()), (15.0, 45.0, 2));
    }
}
