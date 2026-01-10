use crossbeam::queue::SegQueue;
use crossbeam_channel::{Receiver, Sender};

use crate::chunk_loader::ChunkLoaderJob;

const JOB_TYPE_COUNT: usize = 2;

#[derive(Copy, Clone, Debug)]
pub enum JobType {
    Generation,
    Meshing,
}

impl JobType {
    fn index(self) -> usize {
        match self {
            JobType::Meshing => 0,
            JobType::Generation => 1,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct JobPriority {
    pub distance_in_chunks: u32,
    pub job_type: JobType,
}

pub struct LoaderJobQueue {
    queues: Box<[[SegQueue<ChunkLoaderJob>; JOB_TYPE_COUNT]]>,
    job_available_sender: Sender<()>,
    job_available_receiver: Receiver<()>,
}

impl LoaderJobQueue {
    pub fn new(max_distance_in_chunks: u32) -> Self {
        // Capacity 1: coalesce multiple pushes into a single wakeup.
        let (job_available_sender, job_available_receiver) = crossbeam_channel::bounded(1);

        let mut queues = Vec::with_capacity(max_distance_in_chunks as usize + 1);
        for _ in 0..=max_distance_in_chunks {
            queues.push([SegQueue::new(), SegQueue::new()]);
        }

        LoaderJobQueue {
            queues: queues.into_boxed_slice(),
            job_available_sender,
            job_available_receiver,
        }
    }

    pub fn subscribe(&self) -> Receiver<()> {
        self.job_available_receiver.clone()
    }

    pub fn push(&self, job: ChunkLoaderJob, priority: JobPriority) {
        let max_index = (self.queues.len() - 1) as u32;
        let distance_index = priority.distance_in_chunks.min(max_index) as usize;
        self.queues[distance_index][priority.job_type.index()].push(job);
        let _ = self.job_available_sender.try_send(());
    }

    pub fn push_batch(&self, jobs: impl IntoIterator<Item = (ChunkLoaderJob, JobPriority)>) {
        let max_index = (self.queues.len() - 1) as u32;
        let mut pushed_any = false;

        for (job, priority) in jobs {
            let distance_index = priority.distance_in_chunks.min(max_index) as usize;
            self.queues[distance_index][priority.job_type.index()].push(job);
            pushed_any = true;
        }

        if pushed_any {
            let _ = self.job_available_sender.try_send(());
        }
    }

    pub fn pop(&self) -> Option<ChunkLoaderJob> {
        for queues_for_distance in self.queues.iter() {
            // Always prefer meshing over generation at the same distance.
            if let Some(job) = queues_for_distance[JobType::Meshing.index()].pop() {
                return Some(job);
            }
            if let Some(job) = queues_for_distance[JobType::Generation.index()].pop() {
                return Some(job);
            }
        }

        None
    }

    /// Drains all queues and returns the number of removed jobs.
    pub fn clear(&self) -> usize {
        let mut removed = 0usize;

        for queues_for_distance in self.queues.iter() {
            for queue in queues_for_distance.iter() {
                while let Some(_job) = queue.pop() {
                    removed += 1;
                }
            }
        }

        removed
    }

    pub fn is_empty(&self) -> bool {
        for queues_for_distance in self.queues.iter() {
            if !queues_for_distance[0].is_empty() || !queues_for_distance[1].is_empty() {
                return false;
            }
        }
        true
    }
}
