import type { CollaborationError, NodeStatus } from "../types/collaboration";

export const collaborationErrors: Record<CollaborationError, string> = {
  invalid: "输入或信任确认无效；请核对公开描述、指纹、地址和邀请。",
  notConfigured: "协作尚未配置；请先明确选择托管或加入。",
  incomplete: "本机 Brain 初始化不完整；保留现场，不能覆盖初始化。",
  recoveryRequired: "Node 登记需要恢复；不会再次兑换邀请或覆盖登记。",
  existing: "本机已有协作登记；不能重复创建或切换 Brain。",
  storage: "协作记录不可读取或保存；请保留现场，不要删除文件重试。",
  busy: "另一项协作操作正在进行；请稍后手动读取状态。",
  disconnected: "尚无已认证的 Brain 会话；请显式连接。",
  transport: "连接未完成或已中断；操作结果可能不确定，请先核对状态。",
  credential: "本机协作凭证不可用或已失效；需要恢复，不能自动重新加入。",
  rejected: "Brain 拒绝了操作；请核对成员权限、撤销状态或目录修订。",
  unavailable: "协作状态暂不可用；未自动重试，也未显示底层错误。",
};

export const nodeLabels: Record<NodeStatus["state"], string> = {
  notConfigured: "此 Node 尚未登记",
  recoveryRequired: "此 Node 需要恢复",
  disconnected: "此 Node 已登记，当前未连接",
  connected: "此 Node 已完成认证与对账",
};
