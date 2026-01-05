export type UserRole = "admin" | "user";

export interface PublicUser {
  id: string;
  username: string;
  role: UserRole;
  disabled?: boolean;
  createdAt: string;
}

export interface StoredUser extends PublicUser {
  passwordHash: string;
}

export interface UserStoreData {
  version: 1;
  users: StoredUser[];
}
