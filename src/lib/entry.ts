/** One line in the chat transcript. */
export interface Entry {
  role: "you" | "cat" | "tool" | "error";
  text: string;
  /** Tool entries only: the tool's name. */
  tool?: string;
  /** Tool entries only: undefined while the call is still running. */
  ok?: boolean;
}
