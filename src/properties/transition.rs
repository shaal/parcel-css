use cssparser::*;
use crate::traits::{Parse, ToCss, PropertyHandler};
use crate::values::{time::Time, easing::EasingFunction};
use super::{Property, PropertyId};
use crate::vendor_prefix::VendorPrefix;
use crate::declaration::DeclarationList;
use crate::printer::Printer;
use itertools::izip;
use smallvec::SmallVec;
use crate::targets::Browsers;
use crate::prefixes::Feature;
use crate::error::{ParserError, PrinterError};
use crate::logical::LogicalProperties;
use crate::compat;

/// https://www.w3.org/TR/2018/WD-css-transitions-1-20181011/#transition-shorthand-property
#[derive(Debug, Clone, PartialEq)]
pub struct Transition<'i> {
  pub property: PropertyId<'i>,
  pub duration: Time,
  pub delay: Time,
  pub timing_function: EasingFunction 
}

impl<'i> Parse<'i> for Transition<'i> {
  fn parse<'t>(input: &mut Parser<'i, 't>) -> Result<Self, ParseError<'i, ParserError<'i>>> {
    let mut property = None;
    let mut duration = None;
    let mut delay = None;
    let mut timing_function = None;

    loop {
      if duration.is_none() {
        if let Ok(value) = input.try_parse(Time::parse) {
          duration = Some(value);
          continue
        }
      }

      if timing_function.is_none() {
        if let Ok(value) = input.try_parse(EasingFunction::parse) {
          timing_function = Some(value);
          continue
        }
      }

      if delay.is_none() {
        if let Ok(value) = input.try_parse(Time::parse) {
          delay = Some(value);
          continue
        }
      }

      if property.is_none() {
        if let Ok(value) = input.try_parse(PropertyId::parse) {
          property = Some(value);
          continue
        }
      }

      break
    }

    Ok(Transition {
      property: property.unwrap_or(PropertyId::All),
      duration: duration.unwrap_or(Time::Seconds(0.0)),
      delay: delay.unwrap_or(Time::Seconds(0.0)),
      timing_function: timing_function.unwrap_or(EasingFunction::Ease)
    })
  }
}

impl<'i> ToCss for Transition<'i> {
  fn to_css<W>(&self, dest: &mut Printer<W>) -> Result<(), PrinterError> where W: std::fmt::Write {
    self.property.to_css(dest)?;
    if self.duration != 0.0 || self.delay != 0.0 {
      dest.write_char(' ')?;
      self.duration.to_css(dest)?;
    }

    if self.timing_function != EasingFunction::Ease && self.timing_function != EasingFunction::CubicBezier(0.25, 0.1, 0.25, 1.0) {
      dest.write_char(' ')?;
      self.timing_function.to_css(dest)?;
    }

    if self.delay != 0.0 {
      dest.write_char(' ')?;
      self.delay.to_css(dest)?;
    }

    Ok(())
  }
}

#[derive(Default)]
pub(crate) struct TransitionHandler<'i> {
  targets: Option<Browsers>,
  properties: Option<(SmallVec<[PropertyId<'i>; 1]>, VendorPrefix)>,
  durations: Option<(SmallVec<[Time; 1]>, VendorPrefix)>,
  delays: Option<(SmallVec<[Time; 1]>, VendorPrefix)>,
  timing_functions: Option<(SmallVec<[EasingFunction; 1]>, VendorPrefix)>,
  has_any: bool
}

impl<'i> TransitionHandler<'i> {
  pub fn new(targets: Option<Browsers>) -> Self {
    TransitionHandler {
      targets,
      ..TransitionHandler::default()
    }
  }
}

impl<'i> PropertyHandler<'i> for TransitionHandler<'i> {
  fn handle_property(&mut self, property: &Property<'i>, dest: &mut DeclarationList<'i>, logical: &mut LogicalProperties) -> bool {
    use Property::*;

    macro_rules! maybe_flush {
      ($prop: ident, $val: expr, $vp: ident) => {{
        // If two vendor prefixes for the same property have different
        // values, we need to flush what we have immediately to preserve order.
        if let Some((val, prefixes)) = &self.$prop {
          if val != $val && !prefixes.contains(*$vp) {
            self.flush(dest, logical);
          }
        }
      }};
    }

    macro_rules! property {
      ($feature: ident, $prop: ident, $val: expr, $vp: ident) => {{
        maybe_flush!($prop, $val, $vp);

        // Otherwise, update the value and add the prefix.
        if let Some((val, prefixes)) = &mut self.$prop {
          *val = $val.clone();
          *prefixes |= *$vp;
          if prefixes.contains(VendorPrefix::None) {
            if let Some(targets) = self.targets {
              *prefixes = Feature::$feature.prefixes_for(targets);
            }
          }
        } else {
          let prefixes = if $vp.contains(VendorPrefix::None) {
            if let Some(targets) = self.targets {
              Feature::$feature.prefixes_for(targets)
            } else {
              *$vp
            }
          } else {
            *$vp
          };
          self.$prop = Some(($val.clone(), prefixes));
          self.has_any = true;
        }
      }};
    }

    match property {
      TransitionProperty(val, vp) => property!(TransitionProperty, properties, val, vp),
      TransitionDuration(val, vp) => property!(TransitionDuration, durations, val, vp),
      TransitionDelay(val, vp) => property!(TransitionDelay, delays, val, vp),
      TransitionTimingFunction(val, vp) => property!(TransitionTimingFunction, timing_functions, val, vp),
      Transition(val, vp) => {
        let properties: SmallVec<[PropertyId; 1]> = val.iter().map(|b| b.property.clone()).collect();
        maybe_flush!(properties, &properties, vp);

        let durations: SmallVec<[Time; 1]> = val.iter().map(|b| b.duration.clone()).collect();
        maybe_flush!(durations, &durations, vp);

        let delays: SmallVec<[Time; 1]> = val.iter().map(|b| b.delay.clone()).collect();
        maybe_flush!(delays, &delays, vp);

        let timing_functions: SmallVec<[EasingFunction; 1]> = val.iter().map(|b| b.timing_function.clone()).collect();
        maybe_flush!(timing_functions, &timing_functions, vp);
        
        property!(TransitionProperty, properties, &properties, vp);
        property!(TransitionDuration, durations, &durations, vp);
        property!(TransitionDelay, delays, &delays, vp);
        property!(TransitionTimingFunction, timing_functions, &timing_functions, vp);
      }
      Unparsed(val) if is_transition_property(&val.property_id) => {
        self.flush(dest, logical);
        dest.push(Property::Unparsed(val.get_prefixed(self.targets, Feature::Transition)));
      }
      _ => return false
    }

    true
  }

  fn finalize(&mut self, dest: &mut DeclarationList<'i>, logical: &mut LogicalProperties) {
    self.flush(dest, logical);
  }
}

impl<'i> TransitionHandler<'i> {
  fn flush(&mut self, dest: &mut DeclarationList<'i>, logical_properties: &mut LogicalProperties) {
    if !self.has_any {
      return
    }

    self.has_any = false;

    let mut properties = std::mem::take(&mut self.properties);
    let mut durations = std::mem::take(&mut self.durations);
    let mut delays = std::mem::take(&mut self.delays);
    let mut timing_functions = std::mem::take(&mut self.timing_functions);

    let rtl_properties = if let Some((properties, _)) = &mut properties {
      expand_properties(properties, self.targets, logical_properties)
    } else {
      None
    };

    if let (Some((properties, property_prefixes)), Some((durations, duration_prefixes)), Some((delays, delay_prefixes)), Some((timing_functions, timing_prefixes))) = (&mut properties, &mut durations, &mut delays, &mut timing_functions) {
      // Only use shorthand syntax if the number of transitions matches on all properties.
      let len = properties.len();
      if durations.len() == len && delays.len() == len && timing_functions.len() == len {        
        // Find the intersection of prefixes with the same value.
        // Remove that from the prefixes of each of the properties. The remaining
        // prefixes will be handled by outputing individual properties below.
        let intersection = *property_prefixes & *duration_prefixes & *delay_prefixes & *timing_prefixes;
        if !intersection.is_empty() {
          macro_rules! get_transitions {
            ($properties: ident) => {
              izip!($properties, durations.iter(), delays.iter(), timing_functions.iter()).map(|(property, duration, delay, timing_function)| {
                Transition {
                  property: property.clone(),
                  duration: duration.clone(),
                  delay: delay.clone(),
                  timing_function: timing_function.clone()
                }
              }).collect()
            };
          }
          
          let transitions: SmallVec<[Transition; 1]> = get_transitions!(properties);

          if let Some(rtl_properties) = &rtl_properties {
            let rtl_transitions = get_transitions!(rtl_properties);
            logical_properties.add(
              dest,
              PropertyId::Transition(intersection),
              Property::Transition(transitions, intersection),
              Property::Transition(rtl_transitions, intersection)
            );
          } else {
            dest.push(Property::Transition(transitions.clone(), intersection));
          }

          property_prefixes.remove(intersection);
          duration_prefixes.remove(intersection);
          delay_prefixes.remove(intersection);
          timing_prefixes.remove(intersection);
        }
      }
    }

    if let Some((properties, prefix)) = properties {
      if !prefix.is_empty() {
        if let Some(rtl_properties) = rtl_properties {
          logical_properties.add(
            dest,
            PropertyId::TransitionProperty(prefix),
            Property::TransitionProperty(properties, prefix),
            Property::TransitionProperty(rtl_properties, prefix)
          );
        } else {
          dest.push(Property::TransitionProperty(properties, prefix));
        }
      }
    }

    if let Some((durations, prefix)) = durations {
      if !prefix.is_empty() {
        dest.push(Property::TransitionDuration(durations, prefix));
      }
    }

    if let Some((delays, prefix)) = delays {
      if !prefix.is_empty() {
        dest.push(Property::TransitionDelay(delays, prefix));
      }
    }

    if let Some((timing_functions, prefix)) = timing_functions {
      if !prefix.is_empty() {
        dest.push(Property::TransitionTimingFunction(timing_functions, prefix));
      }
    }

    self.reset();
  }

  fn reset(&mut self) {
    self.properties = None;
    self.durations = None;
    self.delays = None;
    self.timing_functions = None;
  }
}

#[inline]
fn is_transition_property(property_id: &PropertyId) -> bool {
  match property_id {
    PropertyId::TransitionProperty(_) |
    PropertyId::TransitionDuration(_) |
    PropertyId::TransitionDelay(_) |
    PropertyId::TransitionTimingFunction(_) |
    PropertyId::Transition(_) => true,
    _ => false
  }
}

fn expand_properties<'i>(
  properties: &mut SmallVec<[PropertyId<'i>; 1]>,
  targets: Option<Browsers>,
  logical_properties: &mut LogicalProperties
) -> Option<SmallVec<[PropertyId<'i>; 1]>> {
  let mut rtl_properties: Option<SmallVec<[PropertyId; 1]>> = None;
  let len = properties.len();
  let mut i = 0;

  macro_rules! replace {
    ($properties: ident, $props: ident) => {
      $properties[i] = $props[0].clone();
      if $props.len() > 1 {
        $properties.insert_many(i + 1, $props[1..].into_iter().cloned());
      }
    };
  }

  // Expand logical properties in place.
  while i < len {
    match get_logical_properties(&properties[i]) {
      LogicalPropertyId::Block(feature, props) if !logical_properties.is_supported(feature) => {
        replace!(properties, props);
        if let Some(rtl_properties) = &mut rtl_properties {
          replace!(rtl_properties, props);
        }
        i += props.len();
      }
      LogicalPropertyId::Inline(feature, ltr, rtl) if !logical_properties.is_supported(feature) => {
        // Clone properties to create RTL version only when needed.
        if rtl_properties.is_none() {
          rtl_properties = Some(properties.clone());
        }

        replace!(properties, ltr);
        if let Some(rtl_properties) = &mut rtl_properties {
          replace!(rtl_properties, rtl);
        }

        i += ltr.len();
      }
      _ => {
        // Expand vendor prefixes for targets.
        properties[i].set_prefixes_for_targets(targets);
        if let Some(rtl_properties) = &mut rtl_properties {
          rtl_properties[i].set_prefixes_for_targets(targets);
        }
        i += 1;
      }
    }
  }

  rtl_properties
}

enum LogicalPropertyId {
  None,
  Block(compat::Feature, &'static [PropertyId<'static>]),
  Inline(compat::Feature, &'static [PropertyId<'static>], &'static [PropertyId<'static>])
}

#[inline]
fn get_logical_properties(property_id: &PropertyId) -> LogicalPropertyId {
  use PropertyId::*;
  use LogicalPropertyId::*;
  use compat::Feature::*;
  match property_id {
    BlockSize => Block(LogicalSize, &[Height]),
    InlineSize => Inline(LogicalSize, &[Width], &[Height]),
    MinBlockSize => Block(LogicalSize, &[MinHeight]),
    MaxBlockSize => Block(LogicalSize, &[MaxHeight]),
    MinInlineSize => Inline(LogicalSize, &[MinWidth], &[MinHeight]),
    MaxInlineSize => Inline(LogicalSize, &[MaxWidth], &[MaxHeight]),
    
    InsetBlockStart => Block(LogicalInset, &[Top]),
    InsetBlockEnd => Block(LogicalInset, &[Bottom]),
    InsetInlineStart => Inline(LogicalInset, &[Left], &[Right]),
    InsetInlineEnd => Inline(LogicalInset, &[Right], &[Left]),
    InsetBlock => Block(LogicalInset, &[Top, Bottom]),
    InsetInline => Block(LogicalInset, &[Left, Right]),
    Inset => Block(LogicalInset, &[Top, Bottom, Left, Right]),

    MarginBlockStart => Block(LogicalMargin, &[MarginTop]),
    MarginBlockEnd => Block(LogicalMargin, &[MarginBottom]),
    MarginInlineStart => Inline(LogicalMargin, &[MarginLeft], &[MarginRight]),
    MarginInlineEnd => Inline(LogicalMargin, &[MarginRight], &[MarginLeft]),
    MarginBlock => Block(LogicalMargin, &[MarginTop, MarginBottom]),
    MarginInline => Block(LogicalMargin, &[MarginLeft, MarginRight]),

    PaddingBlockStart => Block(LogicalPadding, &[PaddingTop]),
    PaddingBlockEnd => Block(LogicalPadding, &[PaddingBottom]),
    PaddingInlineStart => Inline(LogicalPadding, &[PaddingLeft], &[PaddingRight]),
    PaddingInlineEnd => Inline(LogicalPadding, &[PaddingRight], &[PaddingLeft]),
    PaddingBlock => Block(LogicalPadding, &[PaddingTop, PaddingBottom]),
    PaddingInline => Block(LogicalPadding, &[PaddingLeft, PaddingRight]),

    BorderBlockStart => Block(LogicalBorders, &[BorderTop]),
    BorderBlockStartWidth => Block(LogicalBorders, &[BorderTopWidth]),
    BorderBlockStartColor => Block(LogicalBorders, &[BorderTopColor]),
    BorderBlockStartStyle => Block(LogicalBorders, &[BorderTopStyle]),

    BorderBlockEnd => Block(LogicalBorders, &[BorderBottom]),
    BorderBlockEndWidth => Block(LogicalBorders, &[BorderBottomWidth]),
    BorderBlockEndColor => Block(LogicalBorders, &[BorderBottomColor]),
    BorderBlockEndStyle => Block(LogicalBorders, &[BorderBottomStyle]),

    BorderInlineStart => Inline(LogicalBorders, &[BorderLeft], &[BorderRight]),
    BorderInlineStartWidth => Inline(LogicalBorders, &[BorderLeftWidth], &[BorderRightWidth]),
    BorderInlineStartColor => Inline(LogicalBorders, &[BorderLeftColor], &[BorderRightColor]),
    BorderInlineStartStyle => Inline(LogicalBorders, &[BorderLeftStyle], &[BorderRightStyle]),

    BorderInlineEnd => Inline(LogicalBorders, &[BorderRight], &[BorderLeft]),
    BorderInlineEndWidth => Inline(LogicalBorders, &[BorderRightWidth], &[BorderLeftWidth]),
    BorderInlineEndColor => Inline(LogicalBorders, &[BorderRightColor], &[BorderLeftColor]),
    BorderInlineEndStyle => Inline(LogicalBorders, &[BorderRightStyle], &[BorderLeftStyle]),

    BorderBlock => Block(LogicalBorders, &[BorderTop, BorderBottom]),
    BorderBlockColor => Block(LogicalBorders, &[BorderTopColor, BorderBottomColor]),
    BorderBlockWidth => Block(LogicalBorders, &[BorderTopWidth, BorderBottomWidth]),
    BorderBlockStyle => Block(LogicalBorders, &[BorderTopStyle, BorderBottomStyle]),
    
    BorderInline => Block(LogicalBorders, &[BorderLeft, BorderRight]),
    BorderInlineColor => Block(LogicalBorders, &[BorderLeftColor, BorderRightColor]),
    BorderInlineWidth => Block(LogicalBorders, &[BorderLeftWidth, BorderRightWidth]),
    BorderInlineStyle => Block(LogicalBorders, &[BorderLeftStyle, BorderRightStyle]),

    // Not worth using vendor prefixes for these since border-radius is supported
    // everywhere custom properties (which are used to polyfill logical properties) are.
    BorderStartStartRadius => Inline(LogicalBorders, &[BorderTopLeftRadius(VendorPrefix::None)], &[BorderTopRightRadius(VendorPrefix::None)]),
    BorderStartEndRadius => Inline(LogicalBorders, &[BorderTopRightRadius(VendorPrefix::None)], &[BorderTopLeftRadius(VendorPrefix::None)]),
    BorderEndStartRadius => Inline(LogicalBorders, &[BorderBottomLeftRadius(VendorPrefix::None)], &[BorderBottomRightRadius(VendorPrefix::None)]),
    BorderEndEndRadius => Inline(LogicalBorders, &[BorderBottomRightRadius(VendorPrefix::None)], &[BorderBottomLeftRadius(VendorPrefix::None)]),

    _ => None
  }
}
