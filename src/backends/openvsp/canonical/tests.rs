use super::*;

fn vehicle(ids: [&str; 4]) -> String {
    format!(
        "<Vsp_Geometry>\n  <ParmContainer>\n    <ID>{0}</ID>\n    <Name>Vehicle</Name>\n    <Design>\n      <Length Value=\"8.0\" ID=\"{1}\"/>\n    </Design>\n  </ParmContainer>\n  <AttributeCollection ID=\"{2}\" AttachID=\"{0}\" AttachType=\"2\"/>\n  <Geom>\n    <ParentID>{3}</ParentID>\n    <Name>ABCDEFGHIJ</Name>\n  </Geom>\n</Vsp_Geometry>\n",
        ids[0], ids[1], ids[2], ids[3]
    )
}

#[test]
fn identifiers_are_renamed_by_first_occurrence_and_keep_their_length() {
    let first = vsp3(&vehicle(["YBGVVWMBUT", "QEZGJJUIZMC", "RSTVPDCQD", "NONE"]));
    let second = vsp3(&vehicle(["VDDEGUWHIT", "AHFUGYBBGHN", "BQRGKZBJP", "NONE"]));
    assert_eq!(first, second);
    assert!(first.contains("<ID>AAAAAAAAAA</ID>"));
    assert!(first.contains("ID=\"AAAAAAAAAAB\""));
    assert!(first.contains("AttributeCollection ID=\"AAAAAAAAC\" AttachID=\"AAAAAAAAAA\""));
    assert!(first.contains("<ParentID>NONE</ParentID>"));
    assert!(first.contains("<Name>ABCDEFGHIJ</Name>"));
}

#[test]
fn unsteady_group_components_are_ordered_independently_of_identifier_order() {
    let group = |a: &str, b: &str| {
        format!(
            "<Unsteady_Group>\n  <NumberOfComponents>2</NumberOfComponents>\n  <Component>\n    <CompID>{a}</CompID>\n    <SurfIndex>1</SurfIndex>\n  </Component>\n  <Component>\n    <CompID>{b}</CompID>\n    <SurfIndex>2</SurfIndex>\n  </Component>\n  <ParmContainer>\n    <ID>ZZZZZZZZZZ</ID>\n  </ParmContainer>\n</Unsteady_Group>\n"
        )
    };
    let wing = "<Geom><ID>WINGWINGWI</ID></Geom>\n<Geom><ID>BODYBODYBO</ID></Geom>\n";
    let first = vsp3(&format!("{wing}{}", group("WINGWINGWI", "BODYBODYBO")));
    let second = vsp3(&format!("{wing}{}", group("BODYBODYBO", "WINGWINGWI")));
    assert_ne!(
        first, second,
        "surface indices legitimately differ per component"
    );
    let normalized =
        |text: &str| text.replace("<SurfIndex>1</SurfIndex>", "<SurfIndex>2</SurfIndex>");
    assert_eq!(normalized(&first), normalized(&second));
    assert!(first.contains("<NumberOfComponents>2</NumberOfComponents>\n  <Component>"));
    assert!(first.ends_with("</Unsteady_Group>\n"));
}

#[test]
fn canonical_text_is_a_fixed_point() {
    let once = vsp3(&vehicle(["YBGVVWMBUT", "QEZGJJUIZMC", "RSTVPDCQD", "NONE"]));
    assert_eq!(vsp3(&once), once);
}
