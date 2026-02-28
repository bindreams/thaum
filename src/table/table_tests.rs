use super::*;

#[testutil::test]
fn basic_table() {
    let table = Table::new()
        .col("NAME", Align::Left)
        .col("VALUE", Align::Right)
        .row(&["alpha", "1"])
        .row(&["beta", "23"]);

    let expected = "\
NAME   VALUE
-----  -----
alpha      1
beta      23
";
    assert_eq!(table.to_string(), expected);
}

#[testutil::test]
fn row_grouping() {
    let table = Table::new()
        .col("GROUP", Align::Left)
        .col("ITEM", Align::Left)
        .row(&["a", "x"])
        .row(&["a", "y"])
        .row(&["b", "z"]);

    let expected = "\
GROUP  ITEM
-----  ----
a      x
       y
b      z
";
    assert_eq!(table.to_string(), expected);
}

#[testutil::test]
fn missing_cells() {
    let table = Table::new()
        .col("A", Align::Left)
        .col("B", Align::Right)
        .col("C", Align::Right)
        .row(&["x", "1"])
        .row(&["y"]);

    let expected = "\
A  B  C
-  -  -
x  1
y
";
    assert_eq!(table.to_string(), expected);
}
