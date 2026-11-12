/// Nominal diameters of round ventilation ducts, in mm: the series spiral ducts and their
/// fittings are commonly made in.
pub const ROUND_DIAMETERS_MM: &[f64] =
    &[63.0, 80.0, 100.0, 125.0, 160.0, 200.0, 250.0, 315.0, 355.0, 400.0, 450.0, 500.0, 560.0, 630.0, 710.0, 800.0, 900.0, 1000.0, 1120.0, 1250.0];

/// Cross-section of a round duct of `diameter_mm`, in m².
pub fn round_area_m2(diameter_mm: f64) -> f64 {
    let d = diameter_mm / 1000.0;
    std::f64::consts::PI * d * d / 4.0
}

/// Cross-section of a rectangular duct of `width_mm` by `height_mm`, in m².
pub fn rect_area_m2(width_mm: f64, height_mm: f64) -> f64 {
    width_mm / 1000.0 * (height_mm / 1000.0)
}

/// The diameter, mm, of the round duct that loses the same pressure per metre as a rectangular one
/// of `width_mm` by `height_mm` carrying the same flow (the equal-friction equivalent diameter,
/// from the published formula 1.3 (a·b)^0.625 / (a+b)^0.25).
pub fn equivalent_diameter_mm(width_mm: f64, height_mm: f64) -> f64 {
    1.3 * (width_mm * height_mm).powf(0.625) / (width_mm + height_mm).powf(0.25)
}
