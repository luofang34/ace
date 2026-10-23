use super::*;
use serde_json::json;

fn mass_items() -> BTreeMap<String, MassItem> {
    serde_json::from_value(json!({
        "wing":{"kind":"empty","mass":{"value":10,"unit":"kg"},"cg":[{"value":0,"unit":"m"},{"value":0,"unit":"m"},{"value":0,"unit":"m"}],
            "source":{"kind":"test","reference":"fixture"}}}))
    .unwrap()
}

fn production(learning: f64, quantity: f64) -> Production {
    serde_json::from_value(json!({
        "materials":{"cfrp":{"cost_per_kg":{"value":40,"unit":"USD/kg"},"buy_to_fly":{"value":1.5,"unit":"1"}}},
        "processes":{"layup":{"hours_per_kg":{"value":2,"unit":"h/kg"},"tooling":{"value":8000,"unit":"USD"},
            "learning":{"value":learning,"unit":"1"}}},
        "parts":{"wing":{"mass_item":"wing","material":"cfrp","process":"layup","count":2}},
        "purchased":{"motor":{"unit_cost":{"value":300,"unit":"USD"}}},
        "labor_rate":{"value":50,"unit":"USD/h"},
        "quantity":{"value":quantity,"unit":"1"}}))
    .unwrap()
}

#[test]
fn unit_cost_combines_material_learning_labor_purchases_and_amortized_tooling() {
    let report =
        evaluate_production(&production(1.0, 4.0), &mass_items(), &BTreeMap::new()).unwrap();
    let wing = &report.parts["wing"];
    assert_eq!(wing.finished_mass_kg, 20.0);
    assert_eq!(wing.raw_mass_kg, 30.0);
    assert_eq!(wing.material_usd, 1200.0);
    assert_eq!(wing.first_unit_hours, 40.0);
    assert_eq!(report.first_unit_cost_usd, 1200.0 + 300.0 + 40.0 * 50.0);
    assert_eq!(report.recurring_unit_cost_usd, report.first_unit_cost_usd);
    assert_eq!(report.unit_cost_usd, 3500.0 + 2000.0);
}

#[test]
fn learning_follows_the_wright_cumulative_unit_curve() {
    // On an 80% curve unit 2 takes 80% of unit 1 labor and unit 4 takes 64%.
    let factor = average_learning_factor(0.8, 4);
    let expected = (1.0 + 0.8 + 3.0_f64.powf(0.8_f64.log2()) + 0.64) / 4.0;
    assert!((factor - expected).abs() < 1e-12);
    let small =
        evaluate_production(&production(0.8, 10.0), &mass_items(), &BTreeMap::new()).unwrap();
    let large =
        evaluate_production(&production(0.8, 1000.0), &mass_items(), &BTreeMap::new()).unwrap();
    assert!(large.labor_hours < small.labor_hours);
    assert!(large.unit_cost_usd < small.unit_cost_usd);
    assert_eq!(large.first_unit_cost_usd, small.first_unit_cost_usd);
}

#[test]
fn quantities_rates_and_catalog_values_outside_their_domain_are_rejected() {
    for quantity in [0.0, 2.5, 2e6] {
        assert!(
            evaluate_production(&production(0.9, quantity), &mass_items(), &BTreeMap::new())
                .is_err()
        );
    }
    for learning in [0.0, 1.2] {
        assert!(
            evaluate_production(&production(learning, 5.0), &mass_items(), &BTreeMap::new())
                .is_err()
        );
    }
    let mut thin = production(0.9, 5.0);
    if let Some(material) = thin.materials.get_mut("cfrp") {
        material.buy_to_fly =
            ace_model::Binding::Constant(Quantity::new(0.5, Unit::Ratio).unwrap());
    }
    assert!(evaluate_production(&thin, &mass_items(), &BTreeMap::new()).is_err());
}
