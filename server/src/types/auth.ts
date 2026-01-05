export type UserRole = "admin" | "user";

export interface PublicUser {
  id: string;
  username: string;
  email?: string;
  role: UserRole;
  disabled?: boolean;
  createdAt: string;
}

export interface StoredUser extends PublicUser {
  passwordHash: string;
  // 用于强制撤销历史会话（token 中携带 sv，需与此值匹配）
  sessionVersion?: number;
}

export interface UserStoreData {
  version: 1;
  users: StoredUser[];
}
