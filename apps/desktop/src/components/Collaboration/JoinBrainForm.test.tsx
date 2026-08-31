import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { JoinBrainForm } from "./JoinBrainForm";
const input = vi.hoisted(() => ({
  prepareJoin: vi.fn(),
  parseDescriptor: vi.fn(),
  certificateFingerprint: vi.fn(),
}));
vi.mock("../../lib/collaborationInput", () => input);
afterEach(() => vi.resetAllMocks());

function fill() {
  fireEvent.change(screen.getByLabelText("公开 descriptor JSON"), {
    target: { value: "public" },
  });
  fireEvent.change(screen.getByLabelText("独立渠道取得的完整指纹"), {
    target: { value: "a".repeat(64) },
  });
  fireEvent.click(screen.getByRole("checkbox"));
  fireEvent.change(screen.getByLabelText("一次性邀请 JSON"), {
    target: { value: "private invitation" },
  });
}

it("does not accept the preview fingerprint as independent confirmation", async () => {
  const onJoin = vi.fn();
  input.parseDescriptor.mockReturnValue({
    brainId: "brain-a",
    address: "127.0.0.1:7443",
    serverName: "localhost",
  });
  input.certificateFingerprint.mockResolvedValue("a".repeat(64));
  render(<JoinBrainForm disabled={false} onJoin={onJoin} />);
  fireEvent.change(screen.getByLabelText("公开 descriptor JSON"), {
    target: { value: "public" },
  });
  fireEvent.click(screen.getByRole("button", { name: "预览公开身份" }));
  await screen.findByText("brain-a");
  expect(screen.getByLabelText("独立渠道取得的完整指纹")).toHaveValue("");
  expect(screen.getByRole("checkbox")).not.toBeChecked();
  expect(screen.getByRole("button", { name: "确认信任并加入" })).toBeDisabled();
  expect(screen.getByText("brain-a").closest("dl")).toMatchSnapshot(
    "untrusted identity preview",
  );
  expect(onJoin).not.toHaveBeenCalled();
});

it("clears invitation before validation completes, blocks duplicate submit, and never retries a failure", async () => {
  let reject!: (reason: string) => void;
  input.prepareJoin.mockReturnValue(
    new Promise((_, fail) => {
      reject = fail;
    }),
  );
  const onJoin = vi.fn();
  const { container } = render(
    <JoinBrainForm disabled={false} onJoin={onJoin} />,
  );
  fill();
  fireEvent.submit(container.querySelector("form")!);
  fireEvent.submit(container.querySelector("form")!);
  expect(screen.getByLabelText("一次性邀请 JSON")).toHaveValue("");
  expect(input.prepareJoin).toHaveBeenCalledOnce();
  await act(async () => reject("raw secret detail"));
  expect(screen.getByRole("alert")).toMatchSnapshot("bounded join error");
  expect(screen.queryByText("raw secret detail")).not.toBeInTheDocument();
  expect(onJoin).not.toHaveBeenCalled();
});

it("forwards validated input once and clears confirmation; hidden input never starts enrollment", async () => {
  const params = {
    descriptor: "public",
    confirmedFingerprint: "confirmed",
    invitation: {},
  };
  input.prepareJoin.mockResolvedValue(params);
  const onJoin = vi.fn().mockResolvedValue(null);
  const { container } = render(
    <JoinBrainForm disabled={false} onJoin={onJoin} />,
  );
  fill();
  fireEvent.submit(container.querySelector("form")!);
  await waitFor(() => expect(onJoin).toHaveBeenCalledExactlyOnceWith(params));
  expect(screen.getByRole("checkbox")).not.toBeChecked();
  fill();
  const hidden = vi.spyOn(document, "hidden", "get").mockReturnValue(true);
  fireEvent(document, new Event("visibilitychange"));
  expect(screen.getByLabelText("一次性邀请 JSON")).toHaveValue("");
  fireEvent.submit(container.querySelector("form")!);
  expect(onJoin).toHaveBeenCalledOnce();
  hidden.mockRestore();
});
