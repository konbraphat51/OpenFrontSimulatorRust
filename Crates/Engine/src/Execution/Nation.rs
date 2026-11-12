// SPDX-License-Identifier: AGPL-3.0-only
//! Nation (AI) behaviours and the caches they share.

use crate::Core::PseudoRandom::PseudoRandom;
use std::cell::RefCell;
use std::rc::Rc;

pub mod BotAnnexation;
pub mod NationAllianceBehavior;
pub mod NationBehaviorPorts;
pub mod NationEmojiBehavior;
pub mod NationMIRVBehavior;
pub mod NationNukeBehavior;
pub mod NationStructureBehavior;
pub mod NationUtils;
pub mod NationWarshipBehavior;
pub mod SharedWaterCache;

/// The one `PseudoRandom` a `NationExecution` shares with all its behaviours (upstream passes the
/// same instance, so draws interleave); borrow it only for the duration of a single draw.
pub type SharedRandom = Rc<RefCell<PseudoRandom>>;

/// A behaviour object that several behaviours of one nation hold (upstream shares the instance).
pub type Shared<T> = Rc<RefCell<T>>;
