/**
 * @jest-environment jsdom
 */

import React from "react";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { PendingAction } from "@nebgov/sdk";
import GuardianPage from "../app/guardian/page";

const mockClient = {
  getMembers: jest.fn(),
  getThreshold: jest.fn(),
  getGovernor: jest.fn(),
  getActionTtl: jest.fn(),
  listActions: jest.fn(),
  approveActionWithSign: jest.fn(),
  revokeApprovalWithSign: jest.fn(),
  executeActionWithSign: jest.fn(),
};

jest.mock("@nebgov/sdk", () => ({
  GuardianCouncilClient: jest.fn().mockImplementation(() => mockClient),
}));

const mockToast = { success: jest.fn(), error: jest.fn() };
jest.mock("react-hot-toast", () => ({
  __esModule: true,
  default: { success: (m: string) => mockToast.success(m), error: (m: string) => mockToast.error(m) },
}));

let wallet: { isConnected: boolean; publicKey: string | null; signTransaction: jest.Mock };
jest.mock("../lib/wallet-context", () => ({ useWallet: () => wallet }));
jest.mock("../lib/hooks/useLedgerClock", () => ({
  useLedgerClock: () => ({ currentLedger: 100, isLoading: false, error: null }),
}));

const M1 = "GMEMBERONE";
const M2 = "GMEMBERTWO";
const M3 = "GMEMBERTHREE";
const OUTSIDER = "GOUTSIDER";

function action(overrides: Partial<PendingAction> & { id: bigint }): PendingAction {
  return {
    action: { tag: "Pause" },
    proposer: M1,
    createdLedger: 90,
    approvals: [M1],
    executed: false,
    ...overrides,
  };
}

function connect(publicKey: string | null) {
  wallet = { isConnected: publicKey !== null, publicKey, signTransaction: jest.fn() };
}

describe("GuardianPage", () => {
  beforeEach(() => {
    jest.clearAllMocks();
    process.env.NEXT_PUBLIC_GUARDIAN_COUNCIL_ADDRESS = "CCOUNCIL";
    delete process.env.NEXT_PUBLIC_SIMULATION_ACCOUNT;
    connect(M2);
    mockClient.getMembers.mockResolvedValue([M1, M2, M3]);
    mockClient.getThreshold.mockResolvedValue(2);
    mockClient.getGovernor.mockResolvedValue("CGOVERNOR");
    mockClient.getActionTtl.mockResolvedValue(0);
    mockClient.listActions.mockResolvedValue([
      action({ id: 0n, action: { tag: "Pause" }, approvals: [M1] }),
      action({ id: 1n, action: { tag: "CancelActive", proposalId: 7n }, approvals: [M1, M2] }),
      action({ id: 2n, action: { tag: "Unpause" }, approvals: [M1, M2], executed: true }),
    ]);
    mockClient.approveActionWithSign.mockResolvedValue("hash");
    mockClient.revokeApprovalWithSign.mockResolvedValue("hash");
    mockClient.executeActionWithSign.mockResolvedValue("hash");
  });

  it("explains how to configure the page when no council address is set", () => {
    delete process.env.NEXT_PUBLIC_GUARDIAN_COUNCIL_ADDRESS;
    render(<GuardianPage />);
    expect(screen.getByRole("alert")).toHaveTextContent("Guardian council not configured");
  });

  it("asks a disconnected visitor to connect a wallet", () => {
    connect(null);
    render(<GuardianPage />);
    expect(screen.getByText(/Connect your wallet to view the guardian council/)).toBeInTheDocument();
    expect(mockClient.getMembers).not.toHaveBeenCalled();
  });

  it("shows a loading state while the council loads", () => {
    mockClient.getMembers.mockReturnValue(new Promise(() => undefined));
    render(<GuardianPage />);
    expect(screen.getByRole("status", { name: /loading guardian council/i })).toBeInTheDocument();
  });

  it("shows the error and retries", async () => {
    mockClient.getMembers.mockRejectedValueOnce(new Error("rpc offline"));
    render(<GuardianPage />);
    expect(await screen.findByRole("alert")).toHaveTextContent("rpc offline");

    fireEvent.click(screen.getByRole("button", { name: /try again/i }));
    expect(await screen.findByText("Pending actions")).toBeInTheDocument();
  });

  it("lists members, the threshold and pending actions, hiding executed ones until asked", async () => {
    render(<GuardianPage />);
    expect(await screen.findByText("2 of 3 must approve")).toBeInTheDocument();
    expect(within(screen.getByLabelText("Council members")).getByText(/\(you\)/)).toBeInTheDocument();

    expect(screen.getByText(/#0 · Pause the governor/)).toBeInTheDocument();
    expect(screen.getByText(/#1 · Cancel active proposal #7/)).toBeInTheDocument();
    expect(screen.queryByText(/#2 · Unpause the governor/)).not.toBeInTheDocument();

    const card0 = screen.getByTestId("action-0");
    expect(within(card0).getByText("1 of 2 approvals")).toBeInTheDocument();
    expect(within(card0).getByText("Pending")).toBeInTheDocument();
    expect(within(screen.getByTestId("action-1")).getByText("Ready to execute")).toBeInTheDocument();

    fireEvent.click(screen.getByLabelText(/Show executed \(1\)/));
    expect(screen.getByText(/#2 · Unpause the governor/)).toBeInTheDocument();
    expect(within(screen.getByTestId("action-2")).getByText("Executed")).toBeInTheDocument();
  });

  it("shows an empty state when nothing is pending", async () => {
    mockClient.listActions.mockResolvedValue([]);
    render(<GuardianPage />);
    expect(await screen.findByText("No pending council actions.")).toBeInTheDocument();
  });

  it("lets a member approve an action they have not approved, then refreshes", async () => {
    render(<GuardianPage />);
    const card0 = await screen.findByTestId("action-0");
    expect(within(card0).queryByRole("button", { name: /revoke/i })).not.toBeInTheDocument();
    expect(within(card0).queryByRole("button", { name: "Execute" })).not.toBeInTheDocument();

    fireEvent.click(within(card0).getByRole("button", { name: "Approve" }));
    await waitFor(() =>
      expect(mockClient.approveActionWithSign).toHaveBeenCalledWith(M2, 0n, wallet.signTransaction),
    );
    await waitFor(() => expect(mockToast.success).toHaveBeenCalledWith("Approval submitted."));
    await waitFor(() => expect(mockClient.listActions).toHaveBeenCalledTimes(2));
  });

  it("lets a member revoke their own approval and execute a ready action", async () => {
    render(<GuardianPage />);
    const card1 = await screen.findByTestId("action-1");
    expect(within(card1).queryByRole("button", { name: "Approve" })).not.toBeInTheDocument();

    fireEvent.click(within(card1).getByRole("button", { name: /revoke approval/i }));
    await waitFor(() =>
      expect(mockClient.revokeApprovalWithSign).toHaveBeenCalledWith(M2, 1n, wallet.signTransaction),
    );

    fireEvent.click(await within(screen.getByTestId("action-1")).findByRole("button", { name: "Execute" }));
    await waitFor(() =>
      expect(mockClient.executeActionWithSign).toHaveBeenCalledWith(M2, 1n, wallet.signTransaction),
    );
  });

  it("reports a failed transaction without refreshing", async () => {
    mockClient.approveActionWithSign.mockRejectedValue(new Error("user rejected"));
    render(<GuardianPage />);
    fireEvent.click(within(await screen.findByTestId("action-0")).getByRole("button", { name: "Approve" }));
    await waitFor(() => expect(mockToast.error).toHaveBeenCalledWith("user rejected"));
    expect(mockClient.listActions).toHaveBeenCalledTimes(1);
  });

  it("gives non-members no approve or revoke button, but lets them execute a ready action", async () => {
    connect(OUTSIDER);
    render(<GuardianPage />);
    await screen.findByTestId("action-0");
    expect(screen.queryByRole("button", { name: "Approve" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /revoke/i })).not.toBeInTheDocument();
    expect(within(screen.getByTestId("action-1")).getByRole("button", { name: "Execute" })).toBeInTheDocument();
    expect(within(screen.getByTestId("action-0")).queryByRole("button", { name: "Execute" })).not.toBeInTheDocument();
  });

  it("does not count approvals from members who have since been rotated out", async () => {
    mockClient.listActions.mockResolvedValue([
      action({ id: 0n, approvals: [M1, "GFORMERMEMBER"] }),
    ]);
    render(<GuardianPage />);
    const card = await screen.findByTestId("action-0");
    expect(within(card).getByText("1 of 2 approvals")).toBeInTheDocument();
    expect(within(card).getByText(/\+1 from former member \(not counted\)/)).toBeInTheDocument();
    expect(within(card).getByText("Pending")).toBeInTheDocument();
    expect(within(card).queryByRole("button", { name: "Execute" })).not.toBeInTheDocument();
  });

  it("marks an action expired once its window has passed and offers no actions on it", async () => {
    mockClient.getActionTtl.mockResolvedValue(5);
    mockClient.listActions.mockResolvedValue([action({ id: 0n, createdLedger: 90, approvals: [M1, M3] })]);
    render(<GuardianPage />);
    const card = await screen.findByTestId("action-0");
    expect(within(card).getByText("Expired")).toBeInTheDocument();
    expect(within(card).queryByRole("button")).not.toBeInTheDocument();
  });
});
