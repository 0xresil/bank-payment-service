use uuid::Uuid;

/// Represents a hold on a bank customer's funds within their account.
///
/// This struct should be considered opaque.
///
/// For the sake of simplicity, the amount that is held and the reference
/// to the account aren't tracked anywhere, but you can assume the hold
/// reference contains this information.
#[derive(Debug, Clone, Copy)]
pub struct HoldRef {
    #[allow(dead_code)]
    id: Uuid,
}

/// Client to interact with a remote service that manages customer accounts.
#[async_trait::async_trait]
pub trait AccountService: Clone + Send + Sync + 'static {
    /// Places a hold on the account.
    ///
    /// Reduces the `account_number` account's actual balance by `amount`.
    ///
    /// Placing a hold does NOT remove or transfer money from the account, it
    /// merely prevents the money from being otherwise spent until either
    ///
    /// * the money is removed from the account and sent to the proper recipient
    /// via `withdraw_funds` (in case the payment is concluded);
    /// * the hold is released via `release_hold` and the account holder may
    /// once again spec the money as they wish (in case the payment is canceled)
    ///
    /// In other words, for every call to `place_hold`, there MUST be a matching
    /// call to either `release_hold` or `withdraw_funds`.
    async fn place_hold(&self, account_number: &str, amount: i32) -> Result<HoldRef, String>;

    /// Releases a hold on the account.
    ///
    /// Increases the `account_number` account's actual balance by the amount previously held.
    ///
    /// Typically, this is used when the payment for which the hold was created doesn't go
    /// through fully (it was canceled, the system failed, etc.). Unless holds are released,
    /// a failed payment would mean that the customer wouldn't get the goods (because the merchant
    /// wasn't paid), but wouldn't have access to his money either because a hold is still present
    /// on the funds.
    async fn release_hold(&self, hold_ref: HoldRef) -> Result<(), String>;

    /// Withdraws the held money from the account.
    ///
    /// Decreases the current balance of the account linked to the hold reference by the amount previously held.
    /// The hold on the customer's funds is implicitly released atomically.
    ///
    /// This is the mechanism by which money is transferred out from the customer's account and
    /// into the merchant's account during the settlement process.
    async fn withdraw_funds(&self, hold_ref: HoldRef) -> Result<(), String>;
}

/// A naive implementation of the `Bank.Accounts.Service` behavior.
///
/// This implementation is intended for testing and development only.
///
/// For the sake of simplicity, there's no tracking of account balances,
/// held amounts, etc.: we use "magic" values to trigger unhappy paths
/// instead.
#[derive(Clone, Default)]
pub struct DummyService {
    #[cfg(test)]
    pub response: Option<String>,
}

impl DummyService {
    pub const INVALID_ACCOUNT_NUMBER: &str = "00";
    pub const MIN_VALID_AMOUNT: i32 = 0;
    #[allow(clippy::inconsistent_digit_grouping)]
    pub const MAX_VALID_AMOUNT: i32 = 1_000_000_00;
}

#[async_trait::async_trait]
impl AccountService for DummyService {
    /// Places a hold on the account.
    ///
    /// - If the `account_number` is `DummyService::INVALID_ACCOUNT_NUMBER`, returns `invalid_account_number`.
    /// - If the `amount` is negative, returns `invalid_amount`.
    /// - If the `amount` is greater than `DummyService::MAX_VALID_AMOUNT`, returns `insufficient_funds`.
    ///
    /// Returns `HoldRef` otherwise.
    async fn place_hold(&self, account_number: &str, amount: i32) -> Result<HoldRef, String> {
        #[cfg(test)]
        if let Some(response) = &self.response {
            return Err(response.into());
        }

        if account_number == Self::INVALID_ACCOUNT_NUMBER {
            Err("invalid_account_number".into())
        } else if amount < Self::MIN_VALID_AMOUNT {
            Err("invalid_amount".into())
        } else if amount > Self::MAX_VALID_AMOUNT {
            Err("insufficient_funds".into())
        } else {
            Ok(HoldRef { id: Uuid::new_v4() })
        }
    }

    async fn release_hold(&self, hold_ref: HoldRef) -> Result<(), String> {
        let _ = hold_ref;
        Ok(())
    }

    async fn withdraw_funds(&self, hold_ref: HoldRef) -> Result<(), String> {
        let _ = hold_ref;
        Ok(())
    }
}
