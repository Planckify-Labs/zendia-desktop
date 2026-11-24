#!/usr/bin/env python3
#
# Copyright (c) 2026-present, the Ladybird developers.
#
# SPDX-License-Identifier: BSD-2-Clause

"""Generates the Rust side of the UI's connection to the Hub, for the messages listed in messages.txt.

The Hub's endpoints carry many more C++ types than the Freya UI has use for, so this does not generate the endpoints
whole. It keeps the messages that messages.txt names, under the IDs the C++ side gives them, and the types they carry,
from whichever .ipc file of the tree declares them. A type that no .ipc file declares has its wire form written by hand
in src/ipc_types.rs, and this names each one that is missing there.
"""

import argparse
import io
import re
import sys
import textwrap
from pathlib import Path
from typing import Dict, List, Set, Tuple

SOURCE_ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(SOURCE_ROOT / "Meta" / "Generators"))
sys.path.insert(0, str(SOURCE_ROOT / "Meta" / "Utils"))

import generate_ipc_definitions as ipc  # noqa: E402

# NB: The IPC compiler maps a type without a namespace only if it is built in. These the UI's crate supplies.
UNNAMESPACED_TYPES = ["UnixDateTime", "JsonValue"]
for _name in UNNAMESPACED_TYPES:
    ipc.RUST_BUILTIN_TYPES.setdefault(_name, f"crate::ipc_types::{_name}")

ENDPOINT_FILES = {
    "HubServer": SOURCE_ROOT / "Services" / "Hub" / "HubServer.ipc",
    "HubClient": SOURCE_ROOT / "Services" / "Hub" / "HubClient.ipc",
}


def ipc_files() -> List[Path]:
    files: List[Path] = []
    for directory in ("Libraries", "Services"):
        files += sorted((SOURCE_ROOT / directory).rglob("*.ipc"))
    return files


def read_message_list(path: Path) -> Dict[str, List[str]]:
    messages: Dict[str, List[str]] = {name: [] for name in [*ENDPOINT_FILES, "Types"]}
    endpoint = None
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.split("#", 1)[0].strip()
        if not line:
            continue
        if line.startswith("[") and line.endswith("]"):
            endpoint = line[1:-1]
            if endpoint not in messages:
                raise RuntimeError(f"Unknown endpoint {endpoint} in {path}")
            continue
        if endpoint is None:
            raise RuntimeError(f"Message {line} in {path} comes before any [Endpoint]")
        messages[endpoint].append(line)
    return messages


def type_arguments(cpp_type: str) -> List[str]:
    cpp_type = cpp_type.strip()
    if "<" in cpp_type and cpp_type.endswith(">"):
        return [
            argument
            for argument in ipc.split_template_arguments(cpp_type[cpp_type.index("<") + 1 : -1])
            if not argument.isdecimal()
        ]
    return []


def collect_types(
    cpp_types: List[str], declarations: Dict[str, "ipc.Declaration"]
) -> Tuple[List["ipc.Declaration"], Set[str]]:
    """The declarations the types need, in the order they are first reached, and the types no .ipc file declares."""
    seen: Set[str] = set()
    needed: List[ipc.Declaration] = []
    undeclared: Set[str] = set()

    def visit(cpp_type: str) -> None:
        cpp_type = cpp_type.strip().removeprefix("::")
        arguments = type_arguments(cpp_type)
        if arguments:
            for argument in arguments:
                visit(argument)
            return
        if cpp_type in seen:
            return
        seen.add(cpp_type)
        if cpp_type in UNNAMESPACED_TYPES:
            undeclared.add(cpp_type)
            return
        if cpp_type in ipc.RUST_BUILTIN_TYPES:
            return
        declaration = declarations.get(cpp_type)
        if declaration is None:
            undeclared.add(cpp_type)
            return
        needed.append(declaration)
        if declaration.kind != "enum":
            for member in declaration.members:
                visit(member.type)

    for cpp_type in cpp_types:
        visit(cpp_type)
    return needed, undeclared


def write_endpoint(out: io.StringIO, endpoint: "ipc.Endpoint", wanted: List[str], decodes_messages: bool) -> None:
    names = {message.name for message in endpoint.messages}
    for name in wanted:
        if name not in names:
            raise RuntimeError(f"{endpoint.name} has no message {name}")

    out.write(f"""
#[allow(clippy::all, dead_code)]
pub mod {ipc.rust_field_name(ipc.snake_case(endpoint.name))} {{
    pub const ENDPOINT_MAGIC: u32 = {endpoint.magic};

    pub mod messages {{
""")
    body = io.StringIO()
    variants: List[str] = []
    # NB: The IDs are those the C++ side gives every message of the endpoint, so they count the messages left out.
    message_id = 0
    for message in endpoint.messages:
        message_id += 1
        request_id = message_id
        if message.is_synchronous:
            message_id += 1
        if message.name not in wanted:
            continue
        ipc.write_rust_message_struct(body, message.name, request_id, message.inputs)
        variants.append(ipc.rust_field_name(ipc.pascal_case(message.name)))
        if message.is_synchronous:
            ipc.write_rust_message_struct(body, message.response_name(), message_id, message.outputs)
            request_type = ipc.rust_field_name(ipc.pascal_case(message.name))
            response_type = ipc.rust_field_name(ipc.pascal_case(message.response_name()))
            variants.append(response_type)
            body.write(f"""
    impl ::libipc_rust::wire::SyncMessage for {request_type} {{
        type Response = {response_type};
    }}
""")
    out.write(textwrap.indent(body.getvalue(), "    "))
    out.write("    }\n")

    if decodes_messages and variants:
        out.write("\n    pub enum Message {\n")
        for variant in variants:
            out.write(f"        {variant}(messages::{variant}),\n")
        out.write("""    }

    impl Message {
        /// Decodes a message of this endpoint that the UI handles, or answers `Ok(None)` for one it leaves alone.
        pub fn decode(bytes: &[u8], attachments: ::std::vec::Vec<::libipc_rust::Attachment>) -> ::std::result::Result<::std::option::Option<Self>, ::libipc_rust::wire::DecodeError> {
            let mut decoder = ::libipc_rust::wire::Decoder::new(bytes, attachments);
            let (magic, id) = ::libipc_rust::wire::decode_message_header(&mut decoder)?;
            if magic != ENDPOINT_MAGIC {
                return Err(::libipc_rust::wire::DecodeError::InvalidValue("endpoint magic"));
            }
            let message = match id {
""")
        for variant in variants:
            out.write(
                f"                <messages::{variant} as ::libipc_rust::wire::EndpointMessage>::MESSAGE_ID => "
                f"Self::{variant}(decoder.decode()?),\n"
            )
        out.write("""                _ => return ::std::result::Result::Ok(::std::option::Option::None),
            };
            ::std::result::Result::Ok(::std::option::Option::Some(message))
        }
    }
""")
    out.write("}\n")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--messages", required=True, type=Path, help="The messages the UI sends and receives")
    parser.add_argument("--handwritten", required=True, type=Path, help="The Rust file of the hand-written types")
    parser.add_argument("--output", required=True, type=Path, help="The generated Rust module")
    parser.add_argument("--depfile", type=Path, help="Where to list the files this reads, one per line")
    args = parser.parse_args()

    files = ipc_files()
    documents: Dict[Path, ipc.Document] = {}
    declarations: Dict[str, ipc.Declaration] = {}
    for path in files:
        document = ipc.parse(path.read_text(encoding="utf-8"))
        documents[path] = document
        for declaration in document.declarations:
            declarations.setdefault(declaration.name, declaration)

    wanted = read_message_list(args.messages)
    endpoints: Dict[str, ipc.Endpoint] = {}
    for name, path in ENDPOINT_FILES.items():
        endpoints[name] = next(endpoint for endpoint in documents[path].endpoints if endpoint.name == name)

    carried: List[str] = []
    for name, endpoint in endpoints.items():
        for message in endpoint.messages:
            if message.name in wanted[name]:
                carried += [parameter.type for parameter in message.inputs + message.outputs]
    # NB: [Types] names declared types that only the hand-written types carry.
    needed, undeclared = collect_types(carried + wanted["Types"], declarations)

    handwritten = args.handwritten.read_text(encoding="utf-8")
    missing = sorted(
        cpp_type
        for cpp_type in undeclared
        if not re.search(rf"\b{ipc.rust_field_name(cpp_type.replace('::', '_'))}\b", handwritten)
    )
    if missing:
        raise RuntimeError(f"{args.handwritten.name} needs a wire form for: {', '.join(missing)}")

    out = io.StringIO()
    out.write("// NB: Generated by Zendia/Libraries/HubProtocol/generate.py.\n")
    for declaration in needed:
        name = ipc.rust_field_name(declaration.rust_name())
        if declaration.kind == "enum":
            ipc.write_rust_enum(out, declaration, name)
        elif declaration.kind == "struct":
            ipc.write_rust_struct(out, declaration, name)
        else:
            ipc.write_rust_variant(out, declaration, name)
    # NB: The UI decodes what the Hub tells it, and the answers to what it asks, so both endpoints get a Message enum.
    write_endpoint(out, endpoints["HubServer"], wanted["HubServer"], decodes_messages=False)
    write_endpoint(out, endpoints["HubClient"], wanted["HubClient"], decodes_messages=True)

    args.output.write_text(out.getvalue(), encoding="utf-8")
    if args.depfile:
        inputs = files + [args.messages, Path(ipc.__file__)]
        args.depfile.write_text("\n".join(str(path) for path in inputs) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
