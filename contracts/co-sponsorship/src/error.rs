use soroban_sdk::contracterror;

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum CoSponsorshipError {
    AlreadyInitialized = 1,
    DraftNotFound = 2,
    DraftExpired = 3,
    DraftClosed = 4,
    AlreadyCoSponsored = 5,
    NotCoSponsored = 6,
    CoSponsorLimitReached = 7,
    DraftThresholdNotMet = 8,
    UnauthorizedDraftCreator = 9,
    ZeroVotingPower = 10,
    InvalidVectorLengths = 11,
    NoTargets = 12,
    CalldataTooLarge = 13,
    TooManyCalldataEntries = 14,
    NotInitialized = 15,
    IndexOutOfBounds = 16,
}
