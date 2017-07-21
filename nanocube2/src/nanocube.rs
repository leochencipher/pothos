use cube::Monoid;
use ref_counted_vec::RefCountedVec;
use std;
use std::io::Write;

//////////////////////////////////////////////////////////////////////////////

static DEBUG_ENABLED: bool = true;

macro_rules! debug_print {
    ($str:expr $(,$params:expr)*) => (
        if DEBUG_ENABLED {
            print!("{}:{} - ", file!(), line!());
            println!($str $(,$params)*);
        }
    )
}

macro_rules! debug_var {
    ($var:ident) => (
        debug_print!("{} = {:?}",
                     stringify!($var),
                     $var);
    )
}

//////////////////////////////////////////////////////////////////////////////

fn get_bit(value: usize, bit: usize) -> bool
{
    (value >> bit) & 1 != 0
}

#[derive (Copy, Debug, Clone)]
pub struct NCDimNode {
    pub left: Option<usize>,
    pub right: Option<usize>,
    pub next: Option<usize>
}

pub struct NCDim {
    pub nodes: RefCountedVec<NCDimNode>,
    pub width: usize
}

impl NCDim {
    fn len(&self) -> usize {
        return self.nodes.len();
    }

    fn new(width: usize) -> NCDim {
        NCDim {
            nodes: RefCountedVec::new(),
            width: width
        }
    }
}

impl NCDim {
    pub fn at(&self, index: usize) -> &NCDimNode {
        return &self.nodes.values[index];
    }

    pub fn at_mut(&mut self, index: usize) -> &mut NCDimNode {
        return &mut self.nodes.values[index];
    }
}

pub struct Nanocube<Summary> {
    pub base_root: Option<usize>,
    pub dims: Vec<NCDim>,
    pub summaries: RefCountedVec<Summary>
}

impl <Summary: Monoid + PartialOrd> Nanocube<Summary> {

    pub fn new(widths: Vec<usize>) -> Nanocube<Summary> {
        assert!(widths.len() > 0);
        Nanocube {
            base_root: None,
            dims: widths.into_iter().map(|w| NCDim::new(w)).collect(),
            summaries: RefCountedVec::new()
        }
    }
    
    pub fn make_node_ref(&mut self, node_index: Option<usize>, dim: usize) -> usize {
        match node_index {
            None => 0,
            Some(v) if dim == self.dims.len() => self.summaries.make_ref(v),
            Some(v) /* otherwise */           => {
                assert!(v < self.dims[dim].len());
                self.dims[dim].nodes.make_ref(v)
            }
        }
    }

    pub fn release_node_ref(&mut self, node_index: Option<usize>, dim: usize) {
        debug_print!("release_node_ref {:?} {}", node_index, dim);
        if let Some(node_index) = node_index {
            let mut stack = Vec::<(usize, usize)>::new();
            fn push(s: &mut Vec<(usize, usize)>, v: (usize, usize)) {
                debug_print!("pushing {:?}", v);
                s.push(v);
            }
            push(&mut stack, (node_index, dim)); 
            while stack.len() > 0 {
                let (node_index, dim) = stack.pop().unwrap();
                if dim == self.dims.len() {
                    self.summaries.release_ref(node_index);
                } else {
                    let new_ref_count = self.dims[dim].nodes.release_ref(node_index);
                    if new_ref_count == 0 {
                        debug_print!("Node {},{} is free", dim, node_index);
                        let mut node = self.dims[dim].at_mut(node_index);
                        if let Some(left) = node.left {
                            push(&mut stack, (left, dim));
                        }
                        if let Some(right) = node.right {
                            push(&mut stack, (right, dim));
                        }
                        if let Some(next) = node.next {
                            push(&mut stack, (next, dim+1));
                        }
                        node.left = None;
                        node.right = None;
                        node.next = None;
                    }
                }
            }
        }
    }
    
    pub fn insert_fresh_node(&mut self,
                             summary: Summary, addresses: &Vec<usize>, dim: usize, bit: usize)
                             -> (Option<usize>, Option<usize>) {
        assert!(self.dims.len() == addresses.len());
        if dim == self.dims.len() {
            let new_ref = self.summaries.insert(summary);
            debug_print!("insert_fresh_node summary {:?} {} {}: {:?}",
                         addresses, dim, bit, Some(new_ref));
            (Some(new_ref), Some(new_ref))
        } else {
            let width = self.dims[dim].width;
            if bit != width {
                let where_to_insert = get_bit(addresses[dim], width-bit-1);
                let recursion_result = self.insert_fresh_node(summary, addresses, dim, bit+1);
                debug_print!("returned from     refinement recurse middle {:?} {} {}: {:?}",
                         addresses, dim, bit, recursion_result);
                let left_recursion_result = recursion_result.0.expect("fresh node should have returned Some");
                let next = self.dims[dim].at(left_recursion_result).next;
                let new_refinement_node = if where_to_insert {
                    NCDimNode {
                        left: None,
                        right: recursion_result.0,
                        next: next
                    }
                } else {
                    NCDimNode {
                        left: recursion_result.0,
                        right: None,
                        next: next
                    }
                };
                self.make_node_ref(next, dim+1);
                self.make_node_ref(recursion_result.0, dim);
                let new_index = self.dims[dim].nodes.insert(new_refinement_node);
                debug_print!("inserted node {:?} at {},{}", new_refinement_node, dim, new_index);
                debug_print!("insert_fresh_node refinement         middle {:?} {} {}: {:?}",
                         addresses, dim, bit, (Some(new_index), recursion_result.1));
                return (Some(new_index), recursion_result.1);
            } else {
                let next_dim_result = self.insert_fresh_node(summary, addresses, dim+1, 0);
                debug_print!("returned from     refinement recurse bottom {:?} {} {}: {:?}",
                         addresses, dim, bit, next_dim_result);
                let new_node_at_current_dim = NCDimNode {
                    left: None,
                    right: None,
                    next: next_dim_result.0
                };
                self.make_node_ref(next_dim_result.0, dim+1);
                let new_index = self.dims[dim].nodes.insert(new_node_at_current_dim);
                debug_print!("inserted node {:?} at {},{}", new_node_at_current_dim, dim, new_index);
                debug_print!("insert_fresh_node refinement         bottom {:?} {} {}: {:?}",
                         addresses, dim, bit, (Some(new_index), next_dim_result.1));
                return (Some(new_index), next_dim_result.1);
            }
        }
    }

    pub fn get_node(&self, dim: usize, node_index: usize) -> NCDimNode {
        self.dims[dim].at(node_index).clone()
    }
    
    pub fn merge(&mut self,
                 node_1: Option<usize>, node_2: Option<usize>,
                 dim: usize) -> (Option<usize>, Option<usize>) {
        debug_print!("Will merge {:?}, {:?} in dim {}",
                 node_1, node_2, dim);
        if let None = node_1 {
            debug_print!("  trivial merge: {:?}", node_2);
            return (node_2, self.get_summary_index(node_2, dim));
        }
        if let None = node_2 {
            debug_print!("  trivial merge: {:?}", node_1);
            return (node_1, self.get_summary_index(node_1, dim));
        }
        let node_1 = node_1.unwrap();
        let node_2 = node_2.unwrap();
        if dim == self.dims.len() {
            let new_summary = {
                let ref node_1_summary = self.summaries.values[node_1];
                let ref node_2_summary = self.summaries.values[node_2];
                node_1_summary.mapply(node_2_summary)
            };
            let new_summary_index = self.summaries.insert(new_summary);
            debug_print!("  summary insert: {:?}", new_summary_index);
            return (Some(new_summary_index), Some(new_summary_index));
        }
        debug_print!("  nontrivial merge");
        let node_1_node = self.get_node(dim, node_1);
        let node_2_node = self.get_node(dim, node_2);
        debug_print!("  left side of nontrivial merge");
        let left_merge = self.merge(node_1_node.left, node_2_node.left, dim);
        debug_print!("  right side of nontrivial merge");
        let right_merge = self.merge(node_1_node.right, node_2_node.right, dim);
        let node_1_index = left_merge.0;
        let node_2_index = right_merge.0;
        debug_print!("  next side of nontrivial merge");
        let next_merge = match (node_1_index, node_2_index) {
            (None, None) =>
                self.merge(node_1_node.next, node_2_node.next, dim+1),
            (None, Some(node_2_merge_next)) =>
                (self.dims[dim].at(node_2_merge_next).next, right_merge.1),
            (Some(node_1_merge_next), None) =>
                (self.dims[dim].at(node_1_merge_next).next, left_merge.1),
            (Some(node_1_merge_next), Some(node_2_merge_next)) => {
                let node_1_merge_next_next = self.get_node(dim, node_1_merge_next).next;
                let node_2_merge_next_next = self.get_node(dim, node_2_merge_next).next;
                self.merge(node_1_merge_next_next,
                           node_2_merge_next_next,
                           dim+1)
            }
        };
        let new_node = NCDimNode {
            left: left_merge.0,
            right: right_merge.0,
            next: next_merge.0
        };
        self.make_node_ref(left_merge.0, dim);
        self.make_node_ref(right_merge.0, dim);
        self.make_node_ref(next_merge.0, dim+1);
        let new_index = self.dims[dim].nodes.insert(new_node);
        debug_print!("  Inserted merge node {:?} at {},{}",
                 new_node, dim, new_index);
        (Some(new_index), next_merge.1)
    }

    pub fn insert(&mut self,
                  summary: Summary, addresses: &Vec<usize>, dim: usize, bit: usize,
                  current_node_index: Option<usize>) ->
        (Option<usize>, Option<usize>)
    // the two return values are:
    // - the index of the merged inserted node in dims[dim];
    // - the index of the "fresh" node in dims[dim];
    {
        if dim == self.dims.len() {
            // we bottomed out; just insert a summary and be done.
            debug_print!("insert bottomed out - inserting into summary array");
            let merged_summary = summary.mapply(&self.summaries.values[current_node_index.unwrap()]);
            let new_summary_index = self.summaries.insert(summary);
            let merged_summary_index = self.summaries.insert(merged_summary);
            debug_print!("  new: {}", new_summary_index);
            debug_print!("  merged: {}", merged_summary_index);
            return (Some(merged_summary_index), Some(new_summary_index));
        }
        if current_node_index == None {
            debug_print!("inserting fresh node {:?} {} {} at {:?}",
                         addresses, dim, bit, current_node_index);
            let fresh_insert = self.insert_fresh_node(summary, addresses, dim, bit);
            debug_print!("--- inserted fresh node.");
            return (fresh_insert.0, fresh_insert.0)
        }
        let current_node_index = current_node_index.unwrap();
        let width = self.dims[dim].width;
        let current_node = self.get_node(dim, current_node_index);
        let current_address = addresses[dim];
        debug_print!("Current node: {} {:?}", current_node_index, current_node);
            
        let (where_to_insert, result) =
            if bit == width {
                // recurse into next dimension through next node
                (None, self.insert(summary, addresses, dim+1, 0, current_node.next))
            } else {
                // recurse into current dimension through refinement nodes
                let w = get_bit(current_address, width-bit-1);
                let refinement_node =
                    if w { current_node.right } else { current_node.left };
                (Some(w), self.insert(summary, addresses, dim, bit+1, refinement_node))
            };

        let new_node = match (current_node.left, current_node.right, where_to_insert) {
            (Some(a), None, Some(false)) => {
                debug_print!("case 1");
                let a_union_c = result.0;
                NCDimNode {
                    left: a_union_c,
                    right: None,
                    next: self.get_node(dim, a_union_c.unwrap()).next
                }
            },
            (Some(a), None, Some(true)) => {
                debug_print!("case 2");
                let c = result.0;
                let n1 = self.get_node(dim, current_node.left.unwrap()).next;
                let n2 = self.get_node(dim, result.0.unwrap()).next;
                let an_union_cn = self.merge(n1, n2, dim+1).0;
                NCDimNode {
                    left: current_node.left,
                    right: c,
                    next: an_union_cn
                }
            },
            (Some(a), None, None) => {
                unreachable!();
            }
            (None, Some(b), Some(false)) => {
                debug_print!("case 3");
                let c = result.0;
                let n1 = self.get_node(dim, current_node.right.unwrap()).next;
                let n2 = self.get_node(dim, result.0.unwrap()).next;
                let bn_union_cn = self.merge(n1, n2, dim+1).0;
                NCDimNode {
                    left: c,
                    right: current_node.right,
                    next: bn_union_cn
                }
            },
            (None, Some(b), Some(true)) => {
                debug_print!("case 4");
                let b_union_c = result.0;
                NCDimNode {
                    left: None,
                    right: b_union_c,
                    next: self.get_node(dim, b_union_c.unwrap()).next
                }
            },
            (None, Some(b), None) => {
                unreachable!();
            },
            (Some(a), Some(b), Some(false)) => {
                debug_print!("case 5");
                let a_union_c = result.0;
                let c = result.1;
                let an_union_bn = current_node.next;
                let cn = self.get_node(dim, c.unwrap()).next;

                // Here's a relatively clever bit. We need the next node to
                // point to (a_next union b_next union c_next).
                //
                // (Eliding _next for what follows,) The
                // straightforward way to do it is to compute (a union c)
                // union b, since we will always have (a union
                // c) from the recursion result, and we have b from the current node.  however,
                // this is inefficient, because the union operation
                // (merge) can be slow if both sides are complex.
                //
                // On the other hand, nanocubes are only well defined
                // for commutative monoids, which means that (a union
                // c) union b = (a union b) union c. So, if we keep
                // track of the freshly inserted node c (which will be
                // a simple structure since it contains a single
                // value), then we union the old (a union b) with the new c
                // and that's equivalent, and faster.
                //
                // And this is why we need to lug result.1 around
                // everywhere.

                // this is relatively fast
                let an_union_bn_union_cn = self.merge(an_union_bn, cn, dim+1).0;

                // this is pretty slow so we don't do it.
                // let an_union_bn_union_cn = self.merge(an_union_cn, bn, dim+1);
                NCDimNode {
                    left: a_union_c,
                    right: current_node.right,
                    next: an_union_bn_union_cn
                }
            },
            (Some(a), Some(b), Some(true)) => {
                debug_print!("case 6");
                // similar logic as the one explained above.
                
                let b_union_c = result.0;
                let c = result.1;
                let an_union_bn = current_node.next;
                let cn = self.get_node(dim, c.unwrap()).next;

                // this is relatively fast
                let an_union_bn_union_cn = self.merge(an_union_bn, cn, dim+1).0;

                // this is pretty slow so we don't do it.
                // let an_union_bn_union_cn = self.merge(an_union_cn, bn, dim+1);
                NCDimNode {
                    left: current_node.left,
                    right: b_union_c,
                    next: an_union_bn_union_cn
                }
            },
            (Some(a), Some(b), None) => {
                unreachable!();
            },
            (None, None, Some(_)) => {
                unreachable!();
            },
            (None, None, None) => {
                debug_print!("case 7");
                NCDimNode {
                    left: None,
                    right: None,
                    next: result.0 // self.get_node(dim, result.0.unwrap()).next
                }
            }
        };
        self.make_node_ref(new_node.left,  dim);
        self.make_node_ref(new_node.right, dim);
        self.make_node_ref(new_node.next,  dim+1);
        let new_index = self.dims[dim].nodes.insert(new_node);
        // self.release_node_ref(result.0, dim);
        debug_print!("insert merge node {:?} at {},{}",
                     new_node, dim, new_index);

        let new_orphan_node = NCDimNode {
            left:  if where_to_insert == Some(false) { result.1 } else { None },
            right: if where_to_insert == Some(true)  { result.1 } else { None },
            next:  if where_to_insert == None { result.1 } else {
                self.get_node(dim, result.1.unwrap()).next
            }
        };
        self.make_node_ref(new_orphan_node.left,  dim);
        self.make_node_ref(new_orphan_node.right, dim);
        self.make_node_ref(new_orphan_node.next,  dim+1);
        let new_orphan_index = self.dims[dim].nodes.insert(new_orphan_node);
        debug_print!("insert 'orphan' node {:?} at {},{}",
                     new_orphan_node, dim, new_orphan_index);
        (Some(new_index), Some(new_orphan_index))
    }

    pub fn add(&mut self,
               summary: Summary,
               addresses: &Vec<usize>)
    {
        let base = self.base_root;
        debug_print!("Will add. {:?}", addresses);
        let (result_base, orphan_node) = self.insert(summary, addresses, 0, 0, base);
        self.make_node_ref(result_base, 0);
        debug_print!("Will release.");
        self.release_node_ref(base, 0);
        debug_print!("-----Done.");
        self.base_root = result_base;

        // This silly ditty is here because the orphan_node is convenient to carry
        // around during insertion, but is actually not needed anywhere else. In
        // order for us to clean it up easily, we first grab a reference to it,
        // and then we release it, so that the refcount gc can do its job.
        self.make_node_ref(orphan_node, 0);
        self.release_node_ref(orphan_node, 0);
    }

    pub fn get_summary_index(&self, node_index: Option<usize>, dim: usize) -> Option<usize> {
        let mut current_node_index = node_index;
        let mut dim = dim;
        while dim < self.dims.len() {
            current_node_index = match current_node_index {
                None => {
                    return None;
                }
                Some(i) => {
                    let next_index = self.dims[dim].at(i).next;
                    dim += 1;
                    next_index
                }
            }
        }
        return current_node_index;
    }

    pub fn report_size(&self) {
        debug_print!("Summary counts: {}", self.summaries.len());
        debug_print!("Dimension counts:");
        for dim in self.dims.iter() {
            debug_print!("  {}", dim.len());
        }
        debug_print!("");
    }

    pub fn debug_print(&self) {
        for ncdim in self.dims.iter() {
            debug_print!("--------");
            for node in ncdim.nodes.values.iter() {
                debug_print!("{:?} {:?} {:?}", node.left, node.right, node.next);
            }
        }
    }
}

//////////////////////////////////////////////////////////////////////////////

fn node_id(i: usize, dim: usize) -> String {
    format!("\"{}_{}\"", i, dim)
}

fn print_dot_ncdim<W: std::io::Write>(w: &mut W, dim: &NCDim, d: usize, draw_next: bool) -> Result<(), std::io::Error>
{
    writeln!(w, " subgraph cluster_{} {{", d).expect("Can't write to w");
    writeln!(w, " label=\"Dim. {}\";", d).expect("Can't write to w");
    for i in 0..dim.nodes.values.len() {
        if dim.nodes.ref_counts[i] == 0 {
            continue;
        }
        let next = match dim.nodes.values[i].next {
            None => format!("{}", "None"),
            Some(s) => format!("{}", s)
        };
        writeln!(w, "  {} [label=\"{}:{} [{}]\"];", node_id(i, d), i, next, dim.nodes.ref_counts[i]).expect("Can't write to w");;
    }
    writeln!(w, "}}").expect("Can't write to w");;
    for i in 0..dim.nodes.values.len() {
        let node = &dim.nodes.values[i];
        if let Some(left) = node.left {
            writeln!(w, "  {} -> {} [label=\"0\"];", node_id(i, d), node_id(left, d)).expect("Can't write to w");;
        }
        if let Some(right) = node.right {
            writeln!(w, "  {} -> {} [label=\"1\"];", node_id(i, d), node_id(right, d)).expect("Can't write to w");;
        }
    }
    Ok(())
}

pub fn print_dot<W: std::io::Write, Summary>(w: &mut W, nc: &Nanocube<Summary>) -> Result<(), std::io::Error>
{
    writeln!(w, "digraph G {{").expect("Can't write to w");
    writeln!(w, "    splines=line;").expect("Can't write to w");;
    for i in 0..nc.dims.len() {
        print_dot_ncdim(w, &nc.dims[i], i, i < (nc.dims.len() - 1)).expect("Can't write to w");
    }
    writeln!(w, "}}").expect("Can't write to w");
    Ok(())
}

//////////////////////////////////////////////////////////////////////////////

pub fn write_dot_to_disk<Summary>(name: &str, nc: &Nanocube<Summary>) -> Result<(), std::io::Error>
{
    let mut f = std::fs::File::create(name)
        .expect("cannot create file");
    print_dot(&mut f, &nc)
}

pub fn write_txt_to_disk<Summary: std::fmt::Debug>(name: &str, nc: &Nanocube<Summary>) -> Result<(), std::io::Error>
{
    let mut f = std::fs::File::create(name)
        .expect("cannot create file");
    for ncdim in nc.dims.iter() {
        writeln!(f, "--------").expect("Can't write to w");
        for node in ncdim.nodes.values.iter() {
            writeln!(f, "{:?} {:?} {:?}", node.left, node.right, node.next).expect("Can't write to w");
        }
    }
    Ok(())
}

pub fn test_nanocube(out: &str, dims: Vec<usize>, data: &Vec<Vec<usize>>) {
    let mut nc = Nanocube::<usize>::new(dims);
    for d in data {
        nc.add(1, d);
    }
    write_dot_to_disk(out, &nc).expect("Couldn't write");
}

pub fn smoke_test()
{
    // test_nanocube("out/out1.dot", vec![2,2],
    //               &vec![vec![0,0],
    //                     vec![3,3]]);
    // test_nanocube("out/out2.dot", vec![2,2],
    //               &vec![vec![0,0],
    //                     vec![1,3]]);
    // test_nanocube("out/out3.dot", vec![2,2],
    //               &vec![vec![0,0],
    //                     vec![0,0],
    //                     vec![0,0]]);
    // test_nanocube("out/out4.dot", vec![2,2],
    //               &vec![vec![0,0],
    //                     vec![0,3]]);
    // test_nanocube("out/out5.dot", vec![2,2],
    //               &vec![vec![0,0],
    //                     vec![0,1]]);
    // test_nanocube("out/out.dot", vec![2,2],
    //               &vec![vec![0,0],
    //                     vec![0,0]]);
    // test_nanocube("out/out1.dot", vec![2,2],
    //               &vec![vec![2,1]]);
    // test_nanocube("out/out2.dot", vec![2,2],
    //               &vec![vec![2,1],
    //                     vec![1,0]]);
    // test_nanocube("out/out0.dot", vec![2,2],
    //               &vec![vec![0,0]]);
    // test_nanocube("out/out1.dot", vec![2,2],
    //               &vec![vec![0,0],
    //                     vec![3,3]]);

    //////////////////////////////////////////////////////////////////////////
    // test_nanocube("/dev/null", vec![2,2],
    //               &vec![vec![0,0],
    //                     vec![1,0],
    //                     vec![1,1]]);
    // test_nanocube("/dev/null", vec![2,2],
    //               &vec![vec![2,1],
    //                     vec![1,0],
    //                     vec![1,1]]);
    //////////////////////////////////////////////////////////////////////////
    
    test_nanocube("out/out0.dot", vec![2,2],
                  &vec![vec![1,0]]);
    test_nanocube("out/out1.dot", vec![2,2],
                  &vec![vec![1,0],
                        vec![1,3]]);
    test_nanocube("out/out2.dot", vec![2,2],
                  &vec![vec![1,0],
                        vec![1,3],
                        vec![1,2]]);

}
