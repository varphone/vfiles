import nodemailer from "nodemailer";

export interface EmailConfig {
  enabled: boolean;
  host: string;
  port: number;
  secure: boolean;
  user?: string;
  pass?: string;
  from: string;
}

export class EmailService {
  private transporter: nodemailer.Transporter | null;
  private from: string;

  constructor(cfg: EmailConfig) {
    this.from = cfg.from;

    if (!cfg.enabled) {
      this.transporter = null;
      return;
    }

    this.transporter = nodemailer.createTransport({
      host: cfg.host,
      port: cfg.port,
      secure: cfg.secure,
      auth: cfg.user ? { user: cfg.user, pass: cfg.pass ?? "" } : undefined,
    });
  }

  isEnabled(): boolean {
    return this.transporter !== null;
  }

  async sendMail(input: {
    to: string;
    subject: string;
    text: string;
    html?: string;
  }): Promise<void> {
    if (!this.transporter) {
      throw new Error("未启用邮件功能");
    }

    await this.transporter.sendMail({
      from: this.from,
      to: input.to,
      subject: input.subject,
      text: input.text,
      html: input.html,
    });
  }
}
