use super::*;

#[test]
fn dimensional_arithmetic_converts_and_rejects_inconsistent_operations() -> ModelResult<()> {
    let span = Quantity::new(35.0, Unit::Meter)?.dimensional()?;
    let area = Quantity::new(100.0, Unit::SquareMeter)?.dimensional()?;
    let ratio = span.power(2)?.multiply(area, true)?.to(Unit::Ratio)?;
    assert!((ratio.value - 12.25).abs() < 1e-12);
    assert!(span.add(area, false).is_err());
    assert!(Quantity::new(f64::NAN, Unit::Meter).is_err());
    assert_eq!(
        Quantity::new(1.0, Unit::NauticalMile)?
            .to(Unit::Meter)?
            .value,
        1852.0
    );
    assert!(
        Quantity::new(1.0, Unit::Dollar)?
            .to(Unit::Kilogram)
            .is_err()
    );
    Ok(())
}

#[test]
fn singular_and_overflowing_expressions_are_errors() -> ModelResult<()> {
    let one = Quantity::new(1.0, Unit::Ratio)?.dimensional()?;
    let zero = Quantity::new(0.0, Unit::Ratio)?.dimensional()?;
    assert!(one.multiply(zero, true).is_err());
    assert!(one.power(17).is_err());
    Ok(())
}
