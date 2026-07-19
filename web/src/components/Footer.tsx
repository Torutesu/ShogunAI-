import { Logo } from './Logo';

export function Footer() {
  return (
    <footer className="footer">
      <div className="container footer__inner">
        <div className="footer__brand">
          <a className="brand" href="/#top">
            <Logo size={22} className="brand__mark" />
            <span className="brand__name">ShogunAI</span>
          </a>
          <p className="t-body-sm muted">
            Memory that captures your day.<br />Execution that acts on it.
          </p>
        </div>
        <div className="footer__cols">
          <div className="footer__col">
            <div className="t-label-sm muted">Product</div>
            <a href="/#memory">Memory</a>
            <a href="/#action">Action</a>
            <a href="/#pricing">Pricing</a>
          </div>
          <div className="footer__col">
            <div className="t-label-sm muted">Company</div>
            <a href="#">About</a>
            <a href="#">Blog</a>
            <a href="#">Careers</a>
          </div>
          <div className="footer__col">
            <div className="t-label-sm muted">Legal</div>
            <a href="#">Privacy</a>
            <a href="#">Terms</a>
            <a href="#">Security</a>
          </div>
        </div>
      </div>
      <div className="container footer__bottom">
        <span className="t-body-sm muted">© 2026 ShogunAI. All rights reserved.</span>
        <span className="t-body-sm muted">Made for the AI-native individual.</span>
      </div>
    </footer>
  );
}
