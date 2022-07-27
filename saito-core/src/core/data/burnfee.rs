//
// our target blocktime
//
pub const HEARTBEAT: u64 = 5_000;

//
// Burn Fee
//
// The burn fee algorithms determine how much ROUTING WORK needs to be in any block
// in order for that block to be valid according to consensus rules. There are two
// functions that are needed.
//
// - determine routing work needed (to produce block)
// - determine burnfee variable (to include in block)
//
// Both of these functions are
pub struct BurnFee {}

impl BurnFee {
    ///
    /// Returns the amount of work needed to produce a block given the timestamp of
    /// the previous block, the current timestamp, and the y-axis of the burn fee
    /// curve. This is used both in the creation of blocks (mempool) as well as
    /// during block validation.
    ///
    /// * `start` - burn fee value (y-axis) for curve determination ("start")
    /// * `current_block_timestamp`- candidate timestamp
    /// * `previous_block_timestamp` - timestamp of previous block
    ///
    pub fn return_routing_work_needed_to_produce_block_in_nolan(
        burn_fee_previous_block: u64,
        current_block_timestamp: u64,
        previous_block_timestamp: u64,
    ) -> u64 {
        //
        // impossible if times misordered
        //
        if previous_block_timestamp >= current_block_timestamp {
            return 10_000_000_000_000_000_000;
        }

        let elapsed_time = match current_block_timestamp - previous_block_timestamp {
            0 => 1,
            diff => diff,
        };

        if elapsed_time >= (2 * HEARTBEAT) {
            return 0;
        }

        // convert to float for division
        let elapsed_time_float = elapsed_time as f64;
        let burn_fee_previous_block_as_float: f64 = burn_fee_previous_block as f64 / 100_000_000.0;
        let work_needed_float: f64 = burn_fee_previous_block_as_float / elapsed_time_float;

        // convert back to nolan for rounding / safety
        (work_needed_float * 100_000_000.0).round() as u64
    }

    /// Returns an adjusted burnfee based on the start value provided
    /// and the difference between the current block timestamp and the
    /// previous block timestamp
    ///
    /// * `start` - The starting burn fee
    /// * `current_block_timestamp` - The timestamp of the current `Block`
    /// * `previous_block_timestamp` - The timestamp of the previous `Block`
    ///
    pub fn return_burnfee_for_block_produced_at_current_timestamp_in_nolan(
        burn_fee_previous_block: u64,
        current_block_timestamp: u64,
        previous_block_timestamp: u64,
    ) -> u64 {
        //
        // impossible if times misordered
        //
        if previous_block_timestamp >= current_block_timestamp {
            return 10_000_000_000_000_000_000;
        }
        let timestamp_difference = match current_block_timestamp - previous_block_timestamp {
            0 => 1,
            diff => diff,
        };

        // algorithm fails if burn fee last block is 0, so default to low value
        if burn_fee_previous_block == 0 {
            return 50_000_000;
        }

        let burn_fee_previous_block_as_float: f64 = burn_fee_previous_block as f64 / 100_000_000.0;

        let res0 = HEARTBEAT as f64 / timestamp_difference as f64;
        let res1 = res0.sqrt();
        let res2: f64 = burn_fee_previous_block_as_float * res1;
        let new_burnfee: u64 = (res2 * 100_000_000.0).round() as u64;

        new_burnfee
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn burnfee_return_work_needed_test() {
        // if our elapsed time is twice our heartbeat, return 0
        assert_eq!(
            BurnFee::return_routing_work_needed_to_produce_block_in_nolan(10, 2 * HEARTBEAT, 0),
            0
        );

        // if their is no difference, the value should be the start value * 10^8
        assert_eq!(
            BurnFee::return_routing_work_needed_to_produce_block_in_nolan(10_0000_0000, 0, 0),
            10_000_000_000_000_000_000,
        );
    }

    #[test]
    fn burnfee_burn_fee_adjustment_test() {
        // if the difference in timestamps is equal to HEARTBEAT, our start value should not change
        let mut new_start_burnfee =
            BurnFee::return_burnfee_for_block_produced_at_current_timestamp_in_nolan(
                100_000_000,
                HEARTBEAT,
                0,
            );
        assert_eq!(new_start_burnfee, 100_000_000);

        // the difference should be the square root of HEARBEAT over the difference in timestamps
        new_start_burnfee =
            BurnFee::return_burnfee_for_block_produced_at_current_timestamp_in_nolan(
                100_000_000,
                HEARTBEAT / 10,
                0,
            );
        assert_eq!(
            new_start_burnfee,
            (100_000_000.0 * (10 as f64).sqrt()).round() as u64
        );
    }
    #[test]
    fn burnfee_slr_match() {
        let burn_fee_previous_block: u64 = 50000000;
        let current_block_timestamp: u64 = 1658821423410;
        let previous_block_timestamp: u64 = 1658821412997;

        let burnfee = BurnFee::return_burnfee_for_block_produced_at_current_timestamp_in_nolan(
            burn_fee_previous_block,
            current_block_timestamp,
            previous_block_timestamp,
        );
        assert_eq!(burnfee, 34647115);
    }
}
