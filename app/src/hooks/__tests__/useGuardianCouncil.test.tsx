/**
 * @jest-environment jsdom
 */

import { act, renderHook, waitFor } from "@testing-library/react";
import type { PendingAction } from "@nebgov/sdk";
import { describeGuardianAction, useGuardianCouncil } from "../useGuardianCouncil";

const mockClient = {
  getMembers: jest.fn(),
  getThreshold: jest.fn(),
  getGovernor: jest.fn(),
  getActionTtl: jest.fn(),
  listActions: jest.fn(),
};

jest.mock("@nebgov/sdk", () => ({
  GuardianCouncilClient: jest.fn().mockImplementation(() => mockClient),
}));

const action: PendingAction = {
  id: 0n,
  action: { tag: "Pause" },
  proposer: "GMEMBER1",
  createdLedger: 10,
  approvals: ["GMEMBER1"],
  executed: false,
};

describe("useGuardianCouncil", () => {
  beforeEach(() => {
    jest.clearAllMocks();
    process.env.NEXT_PUBLIC_GUARDIAN_COUNCIL_ADDRESS = "CCOUNCIL";
    delete process.env.NEXT_PUBLIC_SIMULATION_ACCOUNT;
    mockClient.getMembers.mockResolvedValue(["GMEMBER1", "GMEMBER2"]);
    mockClient.getThreshold.mockResolvedValue(2);
    mockClient.getGovernor.mockResolvedValue("CGOVERNOR");
    mockClient.getActionTtl.mockResolvedValue(500);
    mockClient.listActions.mockResolvedValue([action]);
  });

  it("reports an unconfigured council without calling the contract", async () => {
    delete process.env.NEXT_PUBLIC_GUARDIAN_COUNCIL_ADDRESS;
    const { result } = renderHook(() => useGuardianCouncil("GMEMBER1"));
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.notConfigured).toBe(true);
    expect(result.current.data).toBeNull();
    expect(mockClient.getMembers).not.toHaveBeenCalled();
  });

  it("waits for an account to simulate from", async () => {
    const { result } = renderHook(() => useGuardianCouncil(null));
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.needsAccount).toBe(true);
    expect(result.current.notConfigured).toBe(false);
    expect(mockClient.getMembers).not.toHaveBeenCalled();
  });

  it("falls back to the configured simulation account when no wallet is connected", async () => {
    process.env.NEXT_PUBLIC_SIMULATION_ACCOUNT = "GSIM";
    const { result } = renderHook(() => useGuardianCouncil(null));
    await waitFor(() => expect(result.current.data).not.toBeNull());
    expect(result.current.needsAccount).toBe(false);
  });

  it("loads members, threshold, governor, TTL and actions, and refetches", async () => {
    const { result } = renderHook(() => useGuardianCouncil("GMEMBER1"));
    expect(result.current.loading).toBe(true);

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.error).toBeNull();
    expect(result.current.data).toEqual({
      members: ["GMEMBER1", "GMEMBER2"],
      threshold: 2,
      governor: "CGOVERNOR",
      actionTtl: 500,
      actions: [action],
    });
    expect(mockClient.listActions).toHaveBeenCalledWith(0n, 100);

    mockClient.listActions.mockResolvedValueOnce([]);
    act(() => result.current.refetch());
    await waitFor(() => expect(mockClient.listActions).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(result.current.data?.actions).toEqual([]));
  });

  it("exposes load errors", async () => {
    mockClient.getMembers.mockRejectedValueOnce(new Error("rpc offline"));
    const { result } = renderHook(() => useGuardianCouncil("GMEMBER1"));
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.error).toBe("rpc offline");
    expect(result.current.data).toBeNull();
  });
});

describe("describeGuardianAction", () => {
  it.each([
    [{ tag: "Pause" }, "Pause the governor"],
    [{ tag: "Unpause" }, "Unpause the governor"],
    [{ tag: "CancelActive", proposalId: 4n }, "Cancel active proposal #4"],
    [{ tag: "CancelQueued", proposalId: 9n }, "Cancel queued proposal #9"],
    [{ tag: "RotateMember", old: "GOLD", new: "GNEW" }, "Rotate member GOLD → GNEW"],
    [{ tag: "SetThreshold", threshold: 3 }, "Set approval threshold to 3"],
    [{ tag: "SetActionTtl", ttlLedgers: 0 }, "Disable action expiry"],
    [{ tag: "SetActionTtl", ttlLedgers: 100 }, "Set action expiry to 100 ledgers"],
  ] as const)("describes case %#", (input, expected) => {
    expect(describeGuardianAction(input)).toBe(expected);
  });
});
