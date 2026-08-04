use super::{
    IoError, VAMAS_MAGIC, XpsEnergyKind, XpsExperiment, XpsMeasurement, XpsMeasurementId,
    XpsRegion, XpsRegionId,
};
use std::collections::BTreeMap;

const VAMAS_SENTINEL: f64 = 1.0e36;

struct Header {
    variable_labels: Vec<String>,
    block_count: usize,
    block_start: usize,
}

struct Cursor<'a> {
    lines: &'a [&'a str],
    position: usize,
    block: &'a str,
}

impl<'a> Cursor<'a> {
    fn new(lines: &'a [&'a str], position: usize, block: &'a str) -> Self {
        Self {
            lines,
            position,
            block,
        }
    }

    fn line(&mut self, label: &str) -> Result<&'a str, IoError> {
        let line = self.lines.get(self.position).copied().ok_or_else(|| {
            IoError::InvalidXps(format!(
                "VAMAS block {:?} is truncated before {label}",
                self.block
            ))
        })?;
        self.position += 1;
        Ok(line.trim())
    }

    fn usize(&mut self, label: &str) -> Result<usize, IoError> {
        self.line(label)?.parse().map_err(|_| {
            IoError::InvalidXps(format!(
                "VAMAS block {:?} has an invalid {label}",
                self.block
            ))
        })
    }

    fn f64(&mut self, label: &str) -> Result<f64, IoError> {
        let value = self.line(label)?.parse::<f64>().map_err(|_| {
            IoError::InvalidXps(format!(
                "VAMAS block {:?} has an invalid {label}",
                self.block
            ))
        })?;
        if value.is_finite() {
            Ok(value)
        } else {
            Err(IoError::InvalidXps(format!(
                "VAMAS block {:?} has a non-finite {label}",
                self.block
            )))
        }
    }

    fn skip(&mut self, count: usize, label: &str) -> Result<(), IoError> {
        for _ in 0..count {
            self.line(label)?;
        }
        Ok(())
    }
}

pub fn parse_vamas(text: &str, source: String) -> Result<XpsExperiment, IoError> {
    let lines = text
        .lines()
        .map(|line| line.trim_end_matches('\r'))
        .collect::<Vec<_>>();
    let header = parse_header(&lines)?;
    let starts = block_starts(&lines, &header)?;
    let mut measurements = BTreeMap::<String, XpsMeasurement>::new();
    let mut regions = Vec::new();
    let mut warnings = Vec::new();

    for (index, &start) in starts.iter().enumerate() {
        let end = starts.get(index + 1).copied().unwrap_or(lines.len());
        let block = &lines[start..end];
        match parse_block(
            block,
            &header.variable_labels,
            &mut measurements,
            regions.len(),
        )? {
            Some(region) => regions.push(region),
            None => warnings.push(format!("Skipped non-XPS VAMAS block {}", index + 1)),
        }
    }
    let experiment = XpsExperiment {
        source,
        measurements: measurements.into_values().collect(),
        regions,
        metadata: BTreeMap::new(),
        import_warnings: warnings,
    };
    experiment.validate().map_err(IoError::InvalidXps)?;
    Ok(experiment)
}

fn parse_header(lines: &[&str]) -> Result<Header, IoError> {
    if lines
        .first()
        .is_none_or(|line| !line.starts_with(VAMAS_MAGIC))
    {
        return Err(IoError::InvalidXps("not an ISO 14976 VAMAS file".into()));
    }
    let mut cursor = Cursor::new(lines, 1, "header");
    cursor.skip(4, "header identity")?;
    let comments = cursor.usize("header comment count")?;
    cursor.skip(comments, "header comment")?;
    if cursor.line("experiment mode")? != "NORM" || cursor.line("scan mode")? != "REGULAR" {
        return Err(IoError::InvalidXps(
            "only NORM/REGULAR VAMAS experiments are supported".into(),
        ));
    }
    cursor.line("spectral region count")?;
    let variable_count = cursor.usize("experimental variable count")?;
    let mut variable_labels = Vec::with_capacity(variable_count);
    for _ in 0..variable_count {
        variable_labels.push(cursor.line("experimental variable label")?.to_owned());
        cursor.line("experimental variable unit")?;
    }
    let included = cursor.usize("block inclusion count")?;
    cursor.skip(included, "included block")?;
    let excluded = cursor.usize("block exclusion count")?;
    cursor.skip(excluded, "excluded block")?;
    let future_experiment = cursor.usize("future experiment entry count")?;
    cursor.skip(future_experiment, "future experiment entry")?;
    let future_block_entries = cursor.usize("future block entry count")?;
    if future_block_entries != 0 {
        return Err(IoError::InvalidXps(
            "VAMAS future block entries are not supported".into(),
        ));
    }
    let block_count = cursor.usize("block count")?;
    if block_count == 0 {
        return Err(IoError::InvalidXps("VAMAS file contains no blocks".into()));
    }
    Ok(Header {
        variable_labels,
        block_count,
        block_start: cursor.position,
    })
}

fn block_starts(lines: &[&str], header: &Header) -> Result<Vec<usize>, IoError> {
    let mut starts = Vec::new();
    for index in header.block_start..lines.len().saturating_sub(9) {
        let number = |offset: usize| lines[index + offset].trim().parse::<u16>().ok();
        let calendar_header = number(2).is_some_and(|year| (1900..=2200).contains(&year))
            && number(3).is_some_and(|month| (1..=12).contains(&month))
            && number(4).is_some_and(|day| (1..=31).contains(&day))
            && number(5).is_some_and(|hour| hour <= 23)
            && number(6).is_some_and(|minute| minute <= 59)
            && number(7).is_some_and(|second| second <= 60);
        if calendar_header {
            starts.push(index);
        }
    }
    if starts.len() != header.block_count || starts.first().copied() != Some(header.block_start) {
        return Err(IoError::InvalidXps(format!(
            "VAMAS declares {} blocks but {} trusted block boundaries were found",
            header.block_count,
            starts.len()
        )));
    }
    Ok(starts)
}

fn parse_block(
    block: &[&str],
    variable_labels: &[String],
    measurements: &mut BTreeMap<String, XpsMeasurement>,
    region_index: usize,
) -> Result<Option<XpsRegion>, IoError> {
    let name = block.first().map_or("", |line| line.trim());
    let mut cursor = Cursor::new(block, 1, name);
    let sample_id = cursor.line("sample identifier")?.to_owned();
    cursor.skip(6, "date and time")?;
    cursor.line("GMT offset")?;
    let comment_count = cursor.usize("block comment count")?;
    let comments = (0..comment_count)
        .map(|_| cursor.line("block comment"))
        .collect::<Result<Vec<_>, _>>()?;
    let technique = cursor.line("technique")?;
    if technique != "XPS" {
        return Ok(None);
    }
    let variable_values = variable_labels
        .iter()
        .map(|label| cursor.f64(label))
        .collect::<Result<Vec<_>, _>>()?;
    let source_label = cursor.line("analysis source label")?.to_owned();
    let photon = physical(cursor.f64("analysis source energy")?);
    let source_strength = cursor.f64("analysis source strength")?;
    cursor.skip(4, "analysis source geometry")?;
    let analyser_mode = cursor.line("analyser mode")?.to_owned();
    let pass_energy = physical(cursor.f64("pass energy")?);
    cursor.skip(1, "analyser magnification")?;
    let work_function = present(cursor.f64("work function")?);
    cursor.skip(5, "analyser geometry")?;
    let species = cursor.line("species label")?.to_owned();
    let transition = cursor.line("transition label")?.to_owned();
    cursor.line("species charge")?;
    let energy_label = cursor.line("abscissa label")?.to_owned();
    let energy_unit = cursor.line("abscissa unit")?;
    if !energy_unit.eq_ignore_ascii_case("eV") {
        return Err(IoError::InvalidXps(format!(
            "VAMAS block {name:?} uses unsupported energy unit {energy_unit:?}"
        )));
    }
    let start_ev = cursor.f64("abscissa start")?;
    let step_ev = cursor.f64("abscissa increment")?;
    if step_ev == 0.0 {
        return Err(IoError::InvalidXps(format!(
            "VAMAS block {name:?} has a zero energy increment"
        )));
    }
    let ordinate_count = cursor.usize("ordinate variable count")?;
    if ordinate_count == 0 {
        return Err(IoError::InvalidXps(format!(
            "VAMAS block {name:?} has no ordinate variables"
        )));
    }
    let mut ordinates = Vec::with_capacity(ordinate_count);
    for _ in 0..ordinate_count {
        ordinates.push((
            cursor.line("ordinate label")?.to_owned(),
            cursor.line("ordinate unit")?.to_owned(),
        ));
    }
    let signal_mode = cursor.line("signal mode")?.to_owned();
    let dwell = physical(cursor.f64("dwell time")?);
    let scans_value = cursor.f64("scan count")?;
    let sweeps = (scans_value >= 1.0
        && scans_value <= u32::MAX as f64
        && scans_value.fract().abs() <= f64::EPSILON)
        .then_some(scans_value.round() as u32);
    cursor.skip(4, "sample timing and orientation")?;
    let additional = cursor.usize("additional parameter count")?;
    cursor.skip(
        additional
            .checked_mul(3)
            .ok_or_else(|| IoError::InvalidXps("VAMAS additional parameter overflow".into()))?,
        "additional parameter",
    )?;
    let payload_count = cursor.usize("ordinate value count")?;
    if payload_count % ordinate_count != 0 {
        return Err(IoError::InvalidXps(format!(
            "VAMAS block {name:?} ordinate count is not divisible by its variables"
        )));
    }
    cursor.skip(
        ordinate_count
            .checked_mul(2)
            .ok_or_else(|| IoError::InvalidXps("VAMAS ordinate extrema overflow".into()))?,
        "ordinate extrema",
    )?;
    let payload = (0..payload_count)
        .map(|_| cursor.f64("ordinate value"))
        .collect::<Result<Vec<_>, _>>()?;
    if let Some((offset, value)) = block[cursor.position..]
        .iter()
        .enumerate()
        .find(|(_, line)| {
            let value = line.trim();
            !value.is_empty() && !value.eq_ignore_ascii_case("end of experiment")
        })
    {
        return Err(IoError::InvalidXps(format!(
            "VAMAS block {name:?} contains an unparsed field {:?} at block row {}",
            value.trim(),
            cursor.position + offset + 1
        )));
    }
    let point_count = payload_count / ordinate_count;
    if point_count < 2 {
        return Err(IoError::InvalidXps(format!(
            "VAMAS block {name:?} contains fewer than two points"
        )));
    }
    if let Some(steps) = comment_usize(&comments, "Number Steps :")
        && steps.checked_add(1) != Some(point_count)
    {
        return Err(IoError::InvalidXps(format!(
            "VAMAS block {name:?} point count disagrees with Number Steps"
        )));
    }
    let first_ordinate = payload
        .chunks_exact(ordinate_count)
        .map(|values| values[0])
        .collect::<Vec<_>>();
    let native = (0..point_count)
        .map(|index| start_ev + index as f64 * step_ev)
        .collect::<Vec<_>>();
    let kind = match energy_label.to_ascii_lowercase().as_str() {
        "binding energy" => XpsEnergyKind::Binding,
        "kinetic energy" => XpsEnergyKind::Kinetic,
        _ => {
            return Err(IoError::InvalidXps(format!(
                "VAMAS block {name:?} has unsupported abscissa {energy_label:?}"
            )));
        }
    };
    let binding = match kind {
        XpsEnergyKind::Binding => Some(native.clone()),
        XpsEnergyKind::Kinetic => photon.map(|hv| native.iter().map(|ke| hv - ke).collect()),
    };
    let ordinate_text = format!("{} {}", ordinates[0].0, ordinates[0].1).to_ascii_lowercase();
    let rate = ["cps", "c/s", "count/s", "counts/s", "s-1"]
        .iter()
        .any(|marker| ordinate_text.contains(marker));
    let counted = !rate && signal_mode.to_ascii_lowercase().contains("pulse");
    if !rate && !counted {
        return Err(IoError::InvalidXps(format!(
            "VAMAS block {name:?} does not identify its intensity as counts or a count rate"
        )));
    }
    let divisor = counted
        .then(|| dwell.zip(sweeps))
        .flatten()
        .map(|(dwell, scans)| dwell * f64::from(scans))
        .filter(|value| *value > 0.0);
    if counted && divisor.is_none() {
        return Err(IoError::InvalidXps(format!(
            "VAMAS block {name:?} lacks dwell time or scan count for CPS conversion"
        )));
    }
    let intensity_cps = first_ordinate
        .iter()
        .map(|value| divisor.map_or(*value, |scale| *value / scale))
        .collect();
    let counts = counted.then_some(first_ordinate);
    let position = variable_position(variable_labels, &variable_values)
        .or_else(|| comment_position(&comments));
    let label = comment(&comments, "Location ID :")
        .map(str::to_owned)
        .unwrap_or_else(|| {
            position.map_or_else(
                || sample_id.clone(),
                |position| {
                    format!(
                        "{} @ {:.6}, {:.6}, {:.6} mm",
                        sample_id, position[0], position[1], position[2]
                    )
                },
            )
        });
    let measurement_id = XpsMeasurementId(measurements.len() as u64 + 1);
    let measurement = measurements
        .entry(label.clone())
        .or_insert_with(|| XpsMeasurement {
            id: measurement_id,
            label,
            position_mm: position,
            metadata: metadata_from_comments(
                &comments,
                &[
                    ("sample", "Sample :"),
                    ("charge_neutraliser", "Charge Neutraliser :"),
                    ("filament_current", "Filament Current :"),
                    ("filament_bias", "Filament Bias :"),
                    ("charge_balance", "Charge Balance :"),
                ],
            ),
        })
        .id;
    let mut metadata = metadata_from_comments(
        &comments,
        &[
            ("excitation_mode", "Mode :"),
            ("xray_power", "X-ray Power :"),
            ("lens_mode", "Lens :"),
        ],
    );
    metadata.insert("anode".into(), source_label);
    metadata.insert("analyser_mode".into(), analyser_mode);
    metadata.insert("signal_mode".into(), signal_mode);
    metadata.insert("ordinate_label".into(), ordinates[0].0.clone());
    metadata.insert("ordinate_unit".into(), ordinates[0].1.clone());
    metadata.insert("source_strength".into(), source_strength.to_string());
    if let Some(value) = pass_energy {
        metadata.insert("pass_energy".into(), value.to_string());
    }
    if let Some(value) = work_function {
        metadata.insert("work_function".into(), value.to_string());
    }
    if !species.is_empty() {
        metadata.insert("species".into(), species);
    }
    if !transition.is_empty() {
        metadata.insert("transition".into(), transition);
    }
    Ok(Some(XpsRegion {
        id: XpsRegionId(region_index as u64 + 1),
        measurement,
        name: name.to_owned(),
        native_energy_kind: kind,
        native_energy_ev: native,
        binding_energy_ev: binding,
        intensity_cps,
        counts,
        photon_energy_ev: photon,
        dwell_time_s: dwell,
        sweeps,
        imported_fit: None,
        metadata,
    }))
}

fn physical(value: f64) -> Option<f64> {
    (value.abs() < VAMAS_SENTINEL && value > 0.0).then_some(value)
}

fn present(value: f64) -> Option<f64> {
    (value.abs() < VAMAS_SENTINEL).then_some(value)
}

fn comment<'a>(comments: &'a [&str], prefix: &str) -> Option<&'a str> {
    comments
        .iter()
        .find_map(|line| line.trim().strip_prefix(prefix).map(str::trim))
}

fn comment_usize(comments: &[&str], prefix: &str) -> Option<usize> {
    comment(comments, prefix)?.parse().ok()
}

fn comment_position(comments: &[&str]) -> Option<[f64; 3]> {
    let value = comment(comments, "Description : (")?.strip_suffix(")mm")?;
    let values = value
        .split(',')
        .map(|number| number.trim().parse::<f64>())
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    (values.len() == 3).then(|| [values[0], values[1], values[2]])
}

fn variable_position(labels: &[String], values: &[f64]) -> Option<[f64; 3]> {
    let find = |axis: char| {
        labels
            .iter()
            .position(|label| {
                let normalized = label
                    .to_ascii_lowercase()
                    .chars()
                    .filter(|character| character.is_ascii_alphanumeric())
                    .collect::<String>();
                normalized.starts_with(&format!("position{axis}"))
                    || normalized.starts_with(&format!("{axis}position"))
            })
            .and_then(|index| values.get(index).copied())
    };
    Some([find('x')?, find('y')?, find('z')?])
}

fn metadata_from_comments(comments: &[&str], fields: &[(&str, &str)]) -> BTreeMap<String, String> {
    fields
        .iter()
        .filter_map(|&(key, prefix)| {
            comment(comments, prefix).map(|value| (key.to_owned(), value.to_owned()))
        })
        .collect()
}
