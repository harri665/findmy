"""Interactive Apple sign-in; export only the credentials used by fm-server."""

import argparse
import getpass
import json
import os
from pathlib import Path
import sys
from urllib.parse import urlsplit


def export_session(account):
    """Strip the Apple password and all unrelated tokens from the account export."""
    state = account.to_json()
    data = state["login"]["data"]
    return {
        "dsid": str(data["dsid"]),
        "searchPartyToken": data["mobileme_data"]["tokens"]["searchPartyToken"],
        "uid": state["ids"]["uid"],
        "device_id": state["ids"]["devid"],
    }


def write_session(path, session):
    """Create a private file, refusing to overwrite an existing export."""
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "w", encoding="utf-8") as output:
        json.dump(session, output, indent=2)
        output.write("\n")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--anisette-url", required=True, help="JSON header endpoint of your anisette provider")
    parser.add_argument("--output", type=Path, default=Path("findmy.apple-session.json"))
    args = parser.parse_args(argv)
    url = urlsplit(args.anisette_url)
    if url.scheme not in ("http", "https") or not url.hostname or url.username or url.password or url.fragment:
        parser.error("anisette URL must be HTTP(S), without credentials or a fragment")
    if args.output.exists():
        parser.error("output already exists; choose a new filename with --output")

    from findmy import AppleAccount, LoginState, RemoteAnisetteProvider, SmsSecondFactorMethod

    account = AppleAccount(RemoteAnisetteProvider(args.anisette_url))
    try:
        username = input("Apple account email: ").strip()
        password = getpass.getpass("Apple account password: ")
        state = account.login(username, password)
        if state == LoginState.REQUIRE_2FA:
            methods = account.get_2fa_methods()
            if not methods:
                raise RuntimeError("Apple did not offer a two-factor authentication method")
            for index, method in enumerate(methods, start=1):
                label = f"SMS to {method.phone_number}" if isinstance(method, SmsSecondFactorMethod) else "Trusted Apple device"
                print(f"{index}. {label}")
            while True:
                try:
                    choice = int(input("Choose a verification method: ")) - 1
                    if 0 <= choice < len(methods):
                        break
                except ValueError:
                    pass
                print("Enter one of the method numbers above.")
            method = methods[choice]
            method.request()
            state = method.submit(input("Verification code: ").strip())
        if state != LoginState.LOGGED_IN:
            raise RuntimeError("Apple sign-in did not complete")
        write_session(args.output, export_session(account))
        print(f"Saved {args.output}. Import it in the dashboard's Settings page.")
        return 0
    finally:
        account.close()


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (KeyboardInterrupt, EOFError):
        print("Sign-in cancelled.", file=sys.stderr)
        sys.exit(1)
    except Exception as error:
        # Do not dump third-party exception text, which may include credentials.
        print(f"Sign-in failed ({type(error).__name__}). Check the provider and your account, then try again.", file=sys.stderr)
        sys.exit(1)
