use std::collections::BTreeMap;

use crate::domain::study::StudyDefinition;

pub(super) struct GridGenes<'a> {
    study: &'a StudyDefinition,
    indices: Vec<usize>,
    done: bool,
}

impl<'a> GridGenes<'a> {
    fn new(study: &'a StudyDefinition) -> Self {
        Self {
            study,
            indices: vec![0; study.variables.len()],
            done: false,
        }
    }

    fn advance(&mut self) {
        if self.indices.is_empty() {
            self.done = true;
            return;
        }
        for index in (0..self.indices.len()).rev() {
            let next = self.indices[index].wrapping_add(1);
            if next < self.study.variables[index].values.len() {
                self.indices[index] = next;
                return;
            }
            self.indices[index] = 0;
        }
        self.done = true;
    }
}

impl Iterator for GridGenes<'_> {
    type Item = BTreeMap<String, String>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let genes = self
            .study
            .variables
            .iter()
            .zip(&self.indices)
            .map(|(variable, index)| (variable.id.clone(), variable.values[*index].clone()))
            .collect();
        self.advance();
        Some(genes)
    }
}

pub(super) fn grid_genes(study: &StudyDefinition) -> GridGenes<'_> {
    GridGenes::new(study)
}
